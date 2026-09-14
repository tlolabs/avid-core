use crate::{
    command::{self, os},
    process::{run, RunOptions},
    storage::{self, StagedOutput},
    CancellationToken, Capabilities, Encoding, Error, EventSink, Input, MediaTools, PreviewRequest,
    RenderRequest, Result, Stage, MAX_SOURCE_IMAGE_DIMENSION, MAX_SOURCE_IMAGE_PIXELS,
};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Debug)]
pub struct OperationOptions {
    /// Per probe/capability process; defaults to 30 seconds.
    pub probe_timeout: Option<Duration>,
    /// Per preview process; defaults to 120 seconds.
    pub preview_timeout: Option<Duration>,
    /// Per encoding attempt; defaults to no limit for long media jobs.
    pub render_timeout: Option<Duration>,
}
impl Default for OperationOptions {
    fn default() -> Self {
        Self {
            probe_timeout: Some(Duration::from_secs(30)),
            preview_timeout: Some(Duration::from_secs(120)),
            render_timeout: None,
        }
    }
}
/// Immutable configuration. Operations block the caller; use a dedicated host worker thread.
/// Independent calls can run concurrently. Hosts must serialize calls targeting the same output.
#[derive(Clone, Debug)]
pub struct Renderer {
    tools: MediaTools,
    options: OperationOptions,
}
impl Renderer {
    pub fn new(tools: MediaTools) -> Self {
        Self {
            tools,
            options: OperationOptions::default(),
        }
    }
    pub fn with_options(mut self, options: OperationOptions) -> Self {
        self.options = options;
        self
    }
    pub fn tools(&self) -> &MediaTools {
        &self.tools
    }
    fn probe(
        &self,
        args: &[std::ffi::OsString],
        token: &CancellationToken,
        operation: &'static str,
    ) -> Result<Vec<u8>> {
        run(
            self.tools.ffprobe(),
            args,
            token,
            RunOptions {
                operation,
                timeout: self.options.probe_timeout,
                progress_duration: None,
                parse_progress: false,
                events: &(),
            },
        )
    }
    pub fn probe_audio_duration(
        &self,
        path: &Path,
        token: &CancellationToken,
    ) -> Result<Option<f64>> {
        token.check()?;
        storage::file(path)?;
        let path = storage::absolute(path)?;
        let args = [
            os("-v"),
            os("error"),
            os("-protocol_whitelist"),
            os("file,pipe"),
            os("-select_streams"),
            os("a:0"),
            os("-show_entries"),
            os("stream=duration:format=duration"),
            os("-of"),
            os("default=noprint_wrappers=1:nokey=1"),
            os(path),
        ];
        let output = self.probe(&args, token, "probe audio duration")?;
        Ok(parse_duration(&output))
    }
    pub fn inspect_image(&self, path: &Path, token: &CancellationToken) -> Result<(u32, u32)> {
        token.check()?;
        storage::file(path)?;
        let path = storage::absolute(path)?;
        let args = [
            os("-v"),
            os("error"),
            os("-protocol_whitelist"),
            os("file,pipe"),
            os("-select_streams"),
            os("v:0"),
            os("-show_entries"),
            os("stream=width,height"),
            os("-of"),
            os("csv=p=0:s=x"),
            os(path),
        ];
        let output = self.probe(&args, token, "inspect artwork")?;
        parse_image(&output)
    }
    pub fn capabilities(&self, token: &CancellationToken) -> Result<Capabilities> {
        let output = run(
            self.tools.ffmpeg(),
            &[os("-hide_banner"), os("-encoders")],
            token,
            RunOptions {
                operation: "detect video encoders",
                timeout: self.options.probe_timeout,
                progress_duration: None,
                parse_progress: false,
                events: &(),
            },
        )?;
        Ok(command::parse_capabilities(&String::from_utf8_lossy(
            &output,
        )))
    }
    pub fn preview(
        &self,
        request: &PreviewRequest,
        token: &CancellationToken,
        events: &dyn EventSink,
    ) -> Result<PathBuf> {
        token.check()?;
        events.stage(Stage::Validating);
        crate::model::validate_dimensions(request.settings.width, request.settings.height)?;
        let image = storage::absolute(&request.image)?;
        let output = storage::absolute(&request.output)?;
        let mut protected = request.protected_paths.clone();
        protected.push(image.clone());
        storage::protect(&output, &protected)?;
        events.stage(Stage::Probing);
        self.inspect_image(&image, token)?;
        let staged = StagedOutput::new(&output, ".png")?;
        let args = command::preview(&image, staged.path(), &request.settings);
        events.stage(Stage::Compositing);
        run(
            self.tools.ffmpeg(),
            &args,
            token,
            RunOptions {
                operation: "render preview",
                timeout: self.options.preview_timeout,
                progress_duration: None,
                parse_progress: false,
                events,
            },
        )?;
        token.check()?;
        storage::protect(&output, &protected)?;
        events.stage(Stage::Publishing);
        token.check()?;
        staged.publish(&output)?;
        events.stage(Stage::Complete);
        Ok(request.output.clone())
    }
    pub fn render(
        &self,
        request: &RenderRequest,
        token: &CancellationToken,
        events: &dyn EventSink,
    ) -> Result<PathBuf> {
        token.check()?;
        events.stage(Stage::Validating);
        request.settings.validate()?;
        let mut request = request.clone();
        let original_output = request.output.clone();
        request.output = storage::absolute(&request.output)?;
        let mut images = Vec::new();
        let mut protected = request.protected_paths.clone();
        match &mut request.input {
            Input::Single { image, audio } => {
                *image = storage::absolute(image)?;
                *audio = storage::absolute(audio)?;
                storage::file(image)?;
                storage::file(audio)?;
                images.push(image.clone());
                protected.extend([image.clone(), audio.clone()]);
            }
            Input::Timeline(timeline) => {
                if timeline.clips().is_empty() {
                    return Err(Error::InvalidInput(
                        "Select at least one clip for video export".into(),
                    ));
                }
                if request
                    .output
                    .extension()
                    .and_then(|v| v.to_str())
                    .is_none_or(|v| !v.eq_ignore_ascii_case("mp4"))
                {
                    return Err(Error::InvalidInput(
                        "Timeline export destination must use .mp4".into(),
                    ));
                }
                let mut clips = timeline.clips().to_vec();
                for clip in &mut clips {
                    clip.image = storage::absolute(&clip.image)?;
                    clip.audio = storage::absolute(&clip.audio)?;
                    storage::file(&clip.image)?;
                    storage::file(&clip.audio)?;
                    images.push(clip.image.clone());
                    protected.extend([clip.image.clone(), clip.audio.clone()]);
                }
                *timeline = crate::Timeline::new(clips)?;
            }
        }
        storage::protect(&request.output, &protected)?;
        events.stage(Stage::Probing);
        let mut checked = HashSet::new();
        for image in images {
            if checked.insert(image.clone()) {
                self.inspect_image(&image, token)?;
            }
        }
        let duration = match &request.input {
            Input::Single { audio, .. } => self.probe_audio_duration(audio, token)?,
            Input::Timeline(t) => Some(t.duration_seconds()),
        };
        let capabilities = self.capabilities(token)?;
        let encoder = command::select_encoder(
            request.settings.codec,
            request.settings.encoding,
            &capabilities,
        )?;
        let staged = StagedOutput::new(&request.output, ".mp4")?;
        events.stage(Stage::Compositing);
        let encode = |encoder: &str| -> Result<()> {
            let args = command::export(&request, encoder, staged.path())?;
            events.stage(Stage::Encoding);
            run(
                self.tools.ffmpeg(),
                &args,
                token,
                RunOptions {
                    operation: "encode video",
                    timeout: self.options.render_timeout,
                    progress_duration: duration,
                    parse_progress: true,
                    events,
                },
            )?;
            Ok(())
        };
        if let Err(hardware) = encode(&encoder) {
            let retry = request.settings.encoding == Encoding::Automatic
                && !encoder.starts_with("libx")
                && matches!(hardware, Error::Process { .. });
            if !retry {
                return Err(hardware);
            }
            token.check()?;
            events.diagnostic("Automatic hardware encoding failed; retrying with software");
            let software =
                command::select_encoder(request.settings.codec, Encoding::Software, &capabilities)
                    .and_then(|name| encode(&name));
            if let Err(software) = software {
                if matches!(software, Error::Cancelled) {
                    return Err(software);
                }
                return Err(Error::Fallback {
                    hardware: Box::new(hardware),
                    software: Box::new(software),
                });
            }
        }
        token.check()?;
        storage::protect(&request.output, &protected)?;
        events.stage(Stage::Publishing);
        token.check()?;
        staged.publish(&request.output)?;
        events.stage(Stage::Complete);
        Ok(original_output)
    }
}
fn parse_duration(bytes: &[u8]) -> Option<f64> {
    String::from_utf8_lossy(bytes)
        .lines()
        .filter_map(|line| line.trim().parse::<f64>().ok())
        .find(|v| v.is_finite() && *v > 0.0)
}
fn parse_image(bytes: &[u8]) -> Result<(u32, u32)> {
    let text = String::from_utf8_lossy(bytes);
    let (w, h) = text
        .trim()
        .split_once('x')
        .and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)))
        .ok_or_else(|| Error::InvalidInput("Artwork has no readable dimensions".into()))?;
    if w == 0
        || h == 0
        || w > MAX_SOURCE_IMAGE_DIMENSION
        || h > MAX_SOURCE_IMAGE_DIMENSION
        || u64::from(w) * u64::from(h) > MAX_SOURCE_IMAGE_PIXELS
    {
        return Err(Error::InvalidInput(
            "Artwork exceeds the 32,768-axis / 50-megapixel safety limit or has zero dimensions"
                .into(),
        ));
    }
    Ok((w, h))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn probe_parsing_preserves_duration_fallback_and_limits() {
        assert_eq!(parse_duration(b"N/A\nNaN\n-1\n0\n2.5\n3\n"), Some(2.5));
        assert_eq!(parse_duration(b"inf\nN/A"), None);
        assert_eq!(parse_image(b"640x480\n").unwrap(), (640, 480));
        for bad in ["0x1", "8192x8192", "32769x1", "N/A", "2x2\n2x2"] {
            assert!(parse_image(bad.as_bytes()).is_err());
        }
    }
}
