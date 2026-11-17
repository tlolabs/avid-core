use crate::{
    command,
    storage::{self, StagedOutput},
    CancellationToken, Capabilities, Encoding, Error, EventSink, Input, PreviewRequest, RenderMode,
    RenderRequest, Result, Stage, MAX_SOURCE_IMAGE_DIMENSION, MAX_SOURCE_IMAGE_PIXELS,
};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
    time::{Duration, Instant, SystemTime},
};

#[derive(Clone, Copy, Hash, PartialEq, Eq, Debug)]
struct FileFingerprint {
    len: u64,
    modified: SystemTime,
    #[cfg(unix)]
    dev: u64,
    #[cfg(unix)]
    ino: u64,
    #[cfg(unix)]
    ctime: i64,
    #[cfg(unix)]
    ctime_nsec: i64,
}

impl FileFingerprint {
    #[cfg(unix)]
    fn from_path(path: &Path) -> Option<Self> {
        let meta = std::fs::metadata(path).ok()?;
        let modified = meta.modified().ok()?;
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Some(Self {
            len: meta.len(),
            modified,
            #[cfg(unix)]
            dev: meta.dev(),
            #[cfg(unix)]
            ino: meta.ino(),
            #[cfg(unix)]
            ctime: meta.ctime(),
            #[cfg(unix)]
            ctime_nsec: meta.ctime_nsec(),
        })
    }

    #[cfg(not(unix))]
    fn from_path(_path: &Path) -> Option<Self> {
        // Length/mtime and Windows creation time can survive file replacement.
        // Without a reliable replacement-sensitive identity, probe fresh rather
        // than return dimensions or duration belonging to the previous file.
        None
    }
}

type ProbeEntry<T> = (FileFingerprint, T);

#[derive(Default, Debug)]
struct ProbeCache {
    images: RwLock<HashMap<PathBuf, ProbeEntry<(u32, u32)>>>,
    durations: RwLock<HashMap<PathBuf, ProbeEntry<Option<f64>>>>,
}

impl ProbeCache {
    fn get_image(&self, path: &Path, fp: FileFingerprint) -> Option<(u32, u32)> {
        self.images
            .read()
            .ok()?
            .get(path)
            .and_then(|(cached_fp, value)| (*cached_fp == fp).then_some(*value))
    }
    fn insert_image(&self, path: PathBuf, fp: FileFingerprint, dims: (u32, u32)) {
        if let Ok(mut map) = self.images.write() {
            if map.len() >= 1024 && !map.contains_key(&path) {
                map.clear();
            }
            map.insert(path, (fp, dims));
        }
    }
    fn get_duration(&self, path: &Path, fp: FileFingerprint) -> Option<Option<f64>> {
        self.durations
            .read()
            .ok()?
            .get(path)
            .and_then(|(cached_fp, value)| (*cached_fp == fp).then_some(*value))
    }
    fn insert_duration(&self, path: PathBuf, fp: FileFingerprint, duration: Option<f64>) {
        if let Ok(mut map) = self.durations.write() {
            if map.len() >= 1024 && !map.contains_key(&path) {
                map.clear();
            }
            map.insert(path, (fp, duration));
        }
    }
}

#[derive(Clone, Debug)]
pub struct OperationOptions {
    /// Per linked probe/capability operation; defaults to 30 seconds.
    pub probe_timeout: Option<Duration>,
    /// Per linked preview operation; defaults to 120 seconds.
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
    options: OperationOptions,
    probe_cache: Arc<ProbeCache>,
}
impl Renderer {
    /// Construct the production linked-library renderer. No discovery, downloads or helpers.
    pub fn new() -> Self {
        Self {
            options: OperationOptions::default(),
            probe_cache: Arc::new(ProbeCache::default()),
        }
    }
    pub fn with_options(mut self, options: OperationOptions) -> Self {
        self.options = options;
        self
    }
    fn linked<T>(
        &self,
        token: &CancellationToken,
        timeout: Option<Duration>,
        operation: &'static str,
        duration: Option<f64>,
        events: &dyn EventSink,
        run: impl FnOnce(&avid_media_bridge::Control<'_>) -> avid_media_bridge::Result<T>,
    ) -> Result<T> {
        token.check()?;
        let started = Instant::now();
        let expired = || timeout.is_some_and(|limit| started.elapsed() >= limit);
        let cancelled = || token.is_cancelled() || expired();
        let progress = |seconds: f64| {
            let speed = seconds / started.elapsed().as_secs_f64().max(0.001);
            events.progress(crate::Progress {
                elapsed_seconds: Some(seconds),
                duration_seconds: duration,
                fraction: duration.map(|d| (seconds / d).clamp(0.0, 1.0)),
                eta_seconds: duration.map(|d| (d - seconds).max(0.0) / speed.max(0.001)),
            });
        };
        let result = run(&avid_media_bridge::Control {
            cancelled: &cancelled,
            progress: &progress,
        });
        token.check()?;
        if expired() {
            return Err(Error::Deadline { operation });
        }
        result.map_err(|error| Error::Linked {
            operation,
            code: error.code,
            detail: format!("{}: {}", error.operation, error.detail),
        })
    }
    pub fn probe_audio_duration(
        &self,
        path: &Path,
        token: &CancellationToken,
    ) -> Result<Option<f64>> {
        token.check()?;
        storage::file(path)?;
        let path = storage::absolute(path)?;
        let fp = FileFingerprint::from_path(&path);
        if let Some(fp) = fp {
            if let Some(cached) = self.probe_cache.get_duration(&path, fp) {
                return Ok(cached);
            }
        }
        let info = self.linked(
            token,
            self.options.probe_timeout,
            "probe audio duration",
            None,
            &(),
            |control| avid_media_bridge::probe(&path, control),
        )?;
        if !info.has_audio {
            return Err(Error::InvalidInput(
                "Source contains no audio stream".into(),
            ));
        }
        let duration = info.duration;
        if let Some(fp) = fp {
            self.probe_cache.insert_duration(path, fp, duration);
        }
        Ok(duration)
    }
    pub fn inspect_image(&self, path: &Path, token: &CancellationToken) -> Result<(u32, u32)> {
        token.check()?;
        storage::file(path)?;
        let path = storage::absolute(path)?;
        let fp = FileFingerprint::from_path(&path);
        if let Some(fp) = fp {
            if let Some(cached) = self.probe_cache.get_image(&path, fp) {
                return Ok(cached);
            }
        }
        let info = self.linked(
            token,
            self.options.probe_timeout,
            "inspect artwork",
            None,
            &(),
            |control| avid_media_bridge::probe(&path, control),
        )?;
        let dims = validate_image(info.width, info.height)?;
        if let Some(fp) = fp {
            self.probe_cache.insert_image(path, fp, dims);
        }
        Ok(dims)
    }
    /// Compiled codecs and representative initialization results on this host.
    /// Device availability may change; export still reports actual initialization failure.
    pub fn capabilities(&self, token: &CancellationToken) -> Result<Capabilities> {
        self.encoding_capabilities(token, true)
    }
    fn encoding_capabilities(
        &self,
        token: &CancellationToken,
        initialize_hardware: bool,
    ) -> Result<Capabilities> {
        token.check()?;
        let encoders = self.linked(
            token,
            self.options.probe_timeout,
            "initialize encoder capabilities",
            None,
            &(),
            |control| {
                if initialize_hardware {
                    avid_media_bridge::encoders(control)
                } else {
                    avid_media_bridge::compiled_encoders(control)
                }
            },
        )?;
        token.check()?;
        Ok(Capabilities {
            encoders: encoders
                .into_iter()
                .map(|e| crate::EncoderCapability {
                    codec: e.codec,
                    encoder: e.name,
                    hardware: e.hardware,
                    compiled: true,
                    usable: e.usable,
                    detail: e.detail,
                })
                .collect(),
            filters: [
                "scale",
                "crop",
                "split",
                "gblur",
                "overlay",
                "format",
                "setsar",
                "pad",
                "trim",
                "setpts",
                "atrim",
                "aformat",
                "aresample",
                "asetpts",
                "concat",
                "fps",
                "loop",
                "hflip",
                "vflip",
                "transpose",
            ]
            .into_iter()
            .filter(|name| avid_media_bridge::has_filter(name))
            .map(str::to_owned)
            .collect(),
            linked_version: avid_media_bridge::version(),
        })
    }
    /// Working profile default without reinterpreting an explicit saved choice.
    /// Apple profiles use Automatic (VideoToolbox); software-only builds use Software.
    pub fn default_settings(&self) -> crate::RenderSettings {
        let mut settings = crate::RenderSettings::default();
        let configuration = avid_media_bridge::configuration();
        if configuration.contains("--enable-videotoolbox")
            || !configuration.contains("--enable-libx264")
        {
            settings.encoding = Encoding::Automatic;
        }
        settings
    }
    /// Linked build identity; no executable is inspected or launched.
    pub fn linked_version(&self) -> String {
        avid_media_bridge::version()
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
        let plan = command::linked_preview(&image, staged.path(), &request.settings)?;
        events.stage(Stage::Compositing);
        self.linked(
            token,
            self.options.preview_timeout,
            "render preview",
            None,
            events,
            |control| avid_media_bridge::execute(&plan, control),
        )?;
        token.check()?;
        storage::protect(&output, &protected)?;
        events.stage(Stage::Publishing);
        token.check()?;
        staged.publish(&output, || {
            storage::protect(&output, &protected)?;
            token.check()
        })?;
        events.stage(Stage::Complete);
        Ok(request.output.clone())
    }
    pub fn render(
        &self,
        request: &RenderRequest,
        token: &CancellationToken,
        events: &dyn EventSink,
    ) -> Result<PathBuf> {
        self.render_with_mode(request, RenderMode::PerFrame, token, events)
    }
    /// Simple mode composes artwork sequentially into operation-owned PNGs, then
    /// caches only the output-sized first frame of each clip in FFmpeg memory.
    /// Uses the same validation, staging, progress, cancellation and encoder fallback as render.
    pub fn render_with_mode(
        &self,
        request: &RenderRequest,
        mode: RenderMode,
        token: &CancellationToken,
        events: &dyn EventSink,
    ) -> Result<PathBuf> {
        token.check()?;
        events.stage(Stage::Validating);
        request.settings.validate()?;
        command::validate_linked_timeline(&request.input, request.settings.fps)?;
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
                for (image, audio) in timeline.media_paths_mut() {
                    *image = storage::absolute(image)?;
                    *audio = storage::absolute(audio)?;
                    storage::file(image)?;
                    storage::file(audio)?;
                    images.push(image.clone());
                    protected.extend([image.clone(), audio.clone()]);
                }
            }
        }
        storage::protect(&request.output, &protected)?;
        events.stage(Stage::Probing);
        let mut checked = HashSet::new();
        let unique_images: Vec<_> = images
            .into_iter()
            .filter(|image| checked.insert(image.clone()))
            .collect();
        inspect_images_with(&unique_images, token, &|image, token| {
            self.inspect_image(image, token).map(|_| ())
        })?;
        let duration = match &request.input {
            Input::Single { audio, .. } => self.probe_audio_duration(audio, token)?,
            Input::Timeline(t) => Some(t.duration_seconds()),
        };
        // Explicit software jobs must not initialize unrelated GPU drivers.
        let capabilities =
            self.encoding_capabilities(token, request.settings.encoding != Encoding::Software)?;
        let encoder = command::select_encoder(
            request.settings.codec,
            request.settings.encoding,
            &capabilities,
        )?;
        let staged = StagedOutput::new(&request.output, ".mp4")?;
        events.stage(Stage::Compositing);
        let attempt_started = Instant::now();
        let prepared = if mode == RenderMode::Simple {
            Some(self.prepare_simple_artwork(&mut request, token, events, attempt_started)?)
        } else {
            None
        };
        let first_attempt = std::cell::Cell::new(true);
        let encode = |encoder: &str| -> Result<()> {
            let plan = if mode == RenderMode::Simple {
                command::linked_precomposed_plan(&request, encoder, staged.path(), duration)?
            } else {
                command::linked_plan(&request, encoder, staged.path(), mode, duration)?
            };
            events.stage(Stage::Encoding);
            token.check()?;
            // Artwork preparation belongs to the first attempt's deadline.
            // A hardware-to-software retry retains the documented fresh budget.
            let timeout = if first_attempt.replace(false) {
                remaining_timeout(self.options.render_timeout, attempt_started, "encode video")?
            } else {
                self.options.render_timeout
            };
            self.linked(
                token,
                timeout,
                "encode video",
                duration,
                events,
                |control| avid_media_bridge::execute(&plan, control),
            )?;
            Ok(())
        };
        encode_with_fallback(
            request.settings.codec,
            request.settings.encoding,
            &encoder,
            &capabilities,
            token,
            events,
            encode,
        )?;
        // Native contexts are closed before execute returns. Retire private
        // artwork files before publication/Complete, including on every error.
        drop(prepared);
        token.check()?;
        storage::protect(&request.output, &protected)?;
        events.stage(Stage::Publishing);
        token.check()?;
        staged.publish(&request.output, || {
            storage::protect(&request.output, &protected)?;
            token.check()
        })?;
        events.stage(Stage::Complete);
        Ok(original_output)
    }

    fn prepare_simple_artwork(
        &self,
        request: &mut RenderRequest,
        token: &CancellationToken,
        events: &dyn EventSink,
        attempt_started: Instant,
    ) -> Result<tempfile::TempDir> {
        token.check()?;
        let parent = request.output.parent().ok_or_else(|| {
            Error::InvalidInput("A render destination must have a parent directory".into())
        })?;
        let directory = tempfile::Builder::new()
            .prefix(".avid-composed-")
            .tempdir_in(parent)
            .map_err(|error| Error::io("stage composed artwork", parent, error))?;
        let mut prepared = HashMap::<PathBuf, PathBuf>::new();
        let mut compose = |image: &mut PathBuf| -> Result<()> {
            token.check()?;
            if let Some(path) = prepared.get(image) {
                *image = path.clone();
                return Ok(());
            }
            let output = directory.path().join(format!("{}.png", prepared.len()));
            let plan = command::linked_preview(image, &output, &request.settings)?;
            token.check()?;
            let remaining = remaining_timeout(
                self.options.render_timeout,
                attempt_started,
                "compose artwork",
            )?;
            let timeout = match (self.options.preview_timeout, remaining) {
                (Some(preview), Some(render)) => Some(preview.min(render)),
                (preview, render) => preview.or(render),
            };
            // Internal preparation has no publication or Complete event. Reuse
            // the canonical composition graph and destroy it before the next
            // image, avoiding full-resolution filter scratch for every clip.
            self.linked(token, timeout, "compose artwork", None, events, |control| {
                avid_media_bridge::execute(&plan, control)
            })?;
            let source = image.clone();
            *image = output.clone();
            prepared.insert(source, output);
            Ok(())
        };
        match &mut request.input {
            Input::Single { image, .. } => compose(image)?,
            Input::Timeline(timeline) => {
                for (image, _) in timeline.media_paths_mut() {
                    compose(image)?;
                }
            }
        }
        Ok(directory)
    }
}

fn remaining_timeout(
    timeout: Option<Duration>,
    started: Instant,
    operation: &'static str,
) -> Result<Option<Duration>> {
    timeout
        .map(|limit| {
            limit
                .checked_sub(started.elapsed())
                .filter(|remaining| !remaining.is_zero())
                .ok_or(Error::Deadline { operation })
        })
        .transpose()
}
fn encode_with_fallback(
    codec: crate::Codec,
    mode: Encoding,
    encoder: &str,
    capabilities: &Capabilities,
    token: &CancellationToken,
    events: &dyn EventSink,
    encode: impl Fn(&str) -> Result<()>,
) -> Result<()> {
    token.check()?;
    if let Err(hardware) = encode(encoder) {
        let is_hardware = capabilities
            .encoders
            .iter()
            .any(|e| e.encoder == encoder && e.hardware);
        if mode != Encoding::Automatic || !is_hardware || !matches!(hardware, Error::Linked { .. })
        {
            return Err(hardware);
        }
        token.check()?;
        events.diagnostic("Automatic hardware encoding failed; retrying with software");
        let software = command::select_encoder(codec, Encoding::Software, capabilities)
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
    Ok(())
}

fn inspect_images_with(
    images: &[PathBuf],
    token: &CancellationToken,
    inspect: &(impl Fn(&Path, &CancellationToken) -> Result<()> + Sync),
) -> Result<()> {
    if images.len() <= 1 {
        for image in images {
            token.check()?;
            inspect(image, token)?;
        }
        return Ok(());
    }

    std::thread::scope(|scope| {
        for batch in images.chunks(4) {
            token.check()?;
            let mut workers = Vec::with_capacity(batch.len());
            let mut start_error = None;
            for image in batch {
                if let Err(error) = token.check() {
                    start_error = Some(error);
                    break;
                }
                match std::thread::Builder::new()
                    .name("avid-artwork-probe".into())
                    .spawn_scoped(scope, || inspect(image, token))
                {
                    Ok(worker) => workers.push((image, worker)),
                    Err(error) => {
                        start_error = Some(Error::io("start artwork probe worker", image, error));
                        break;
                    }
                }
            }

            let mut worker_error = None;
            for (image, worker) in workers {
                let result = worker.join().unwrap_or_else(|_| {
                    Err(Error::InvalidInput(format!(
                        "Image probe thread panicked: {}",
                        image.display()
                    )))
                });
                if let Err(error) = result {
                    worker_error.get_or_insert(error);
                }
            }
            if let Some(error) = worker_error.or(start_error) {
                return Err(error);
            }
        }
        Ok(())
    })
}

fn validate_image(w: u32, h: u32) -> Result<(u32, u32)> {
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
impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn cache_fingerprint() -> FileFingerprint {
        FileFingerprint {
            len: 100,
            modified: SystemTime::UNIX_EPOCH,
            #[cfg(unix)]
            dev: 1,
            #[cfg(unix)]
            ino: 2,
            #[cfg(unix)]
            ctime: 3,
            #[cfg(unix)]
            ctime_nsec: 4,
        }
    }

    #[test]
    fn probe_cache_checks_full_identity_and_preserves_unknown_duration() {
        let cache = ProbeCache::default();
        let path = Path::new("artwork");
        let fp = cache_fingerprint();
        cache.insert_image(path.to_owned(), fp, (640, 480));
        cache.insert_duration(path.to_owned(), fp, None);
        assert_eq!(cache.get_image(path, fp), Some((640, 480)));
        assert_eq!(cache.get_duration(path, fp), Some(None));

        let changed = [
            FileFingerprint {
                len: fp.len + 1,
                ..fp
            },
            FileFingerprint {
                modified: fp.modified + Duration::from_nanos(1),
                ..fp
            },
            #[cfg(unix)]
            FileFingerprint {
                dev: fp.dev + 1,
                ..fp
            },
            #[cfg(unix)]
            FileFingerprint {
                ino: fp.ino + 1,
                ..fp
            },
            #[cfg(unix)]
            FileFingerprint {
                ctime: fp.ctime + 1,
                ..fp
            },
            #[cfg(unix)]
            FileFingerprint {
                ctime_nsec: fp.ctime_nsec + 1,
                ..fp
            },
        ];
        for different in changed {
            assert_eq!(cache.get_image(path, different), None);
            assert_eq!(cache.get_duration(path, different), None);
        }

        let replacement = changed[0];
        cache.insert_image(path.to_owned(), replacement, (1920, 1080));
        cache.insert_duration(path.to_owned(), replacement, Some(12.5));
        assert_eq!(cache.get_image(path, fp), None);
        assert_eq!(cache.get_duration(path, fp), None);
        assert_eq!(cache.get_image(path, replacement), Some((1920, 1080)));
        assert_eq!(cache.get_duration(path, replacement), Some(Some(12.5)));
    }

    #[test]
    fn repeated_source_replacements_retain_unrelated_cached_probes_at_capacity() {
        let cache = ProbeCache::default();
        let fp = cache_fingerprint();
        for index in 0..1024 {
            let path = PathBuf::from(format!("artwork-{index}"));
            cache.insert_image(path.clone(), fp, (640, 480));
            cache.insert_duration(path, fp, Some(index as f64));
        }

        let changing = Path::new("artwork-0");
        let unrelated = Path::new("artwork-1023");
        let mut latest = fp;
        for generation in 1..=2048 {
            latest = FileFingerprint {
                len: fp.len + generation,
                ..fp
            };
            cache.insert_image(changing.to_owned(), latest, (1920, 1080));
            cache.insert_duration(changing.to_owned(), latest, None);
        }
        assert_eq!(cache.get_image(unrelated, fp), Some((640, 480)));
        assert_eq!(cache.get_duration(unrelated, fp), Some(Some(1023.0)));
        assert_eq!(cache.get_image(changing, fp), None);
        assert_eq!(cache.get_duration(changing, fp), None);
        assert_eq!(cache.get_image(changing, latest), Some((1920, 1080)));
        assert_eq!(cache.get_duration(changing, latest), Some(None));
        assert_eq!(cache.images.read().unwrap().len(), 1024);
        assert_eq!(cache.durations.read().unwrap().len(), 1024);

        let additional = Path::new("new-artwork");
        cache.insert_image(additional.to_owned(), fp, (1280, 720));
        cache.insert_duration(additional.to_owned(), fp, Some(5.0));
        assert_eq!(cache.images.read().unwrap().len(), 1);
        assert_eq!(cache.durations.read().unwrap().len(), 1);
        assert_eq!(cache.get_image(additional, fp), Some((1280, 720)));
        assert_eq!(cache.get_duration(additional, fp), Some(Some(5.0)));
        assert_eq!(cache.get_image(unrelated, fp), None);
        assert_eq!(cache.get_duration(unrelated, fp), None);
    }

    #[cfg(not(unix))]
    #[test]
    fn probe_cache_is_bypassed_without_replacement_sensitive_identity() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("art.png");
        std::fs::write(&path, b"image").unwrap();
        assert!(std::fs::metadata(&path).unwrap().modified().is_ok());
        assert!(FileFingerprint::from_path(&path).is_none());
    }

    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Completion<'a>(&'a AtomicUsize);
    impl Drop for Completion<'_> {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn linked_dimension_validation_retains_limits() {
        assert_eq!(validate_image(640, 480).unwrap(), (640, 480));
        for (w, h) in [(0, 1), (8192, 8192), (32769, 1)] {
            assert!(validate_image(w, h).is_err());
        }
    }

    #[test]
    fn image_inspection_handles_empty_single_and_cancelled_inputs() {
        let token = CancellationToken::default();
        let calls = AtomicUsize::new(0);
        assert!(inspect_images_with(&[], &token, &|_, _| {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .is_ok());
        assert!(
            inspect_images_with(&[PathBuf::from("one")], &token, &|_, _| {
                calls.fetch_add(1, Ordering::SeqCst);
                Err(Error::InvalidInput("probe failed".into()))
            })
            .is_err()
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        token.cancel();
        assert!(matches!(
            inspect_images_with(&[PathBuf::from("one")], &token, &|_, _| Ok(())),
            Err(Error::Cancelled)
        ));
    }

    #[test]
    fn image_probe_workers_are_bounded_and_all_joined_on_failure_or_panic() {
        use std::{thread, time::Duration};

        let active = AtomicUsize::new(0);
        let maximum = AtomicUsize::new(0);
        let calls = AtomicUsize::new(0);
        let images: Vec<_> = (0..9)
            .map(|index| PathBuf::from(format!("image-{index}")))
            .collect();
        let result = inspect_images_with(&images, &CancellationToken::default(), &|_, _| {
            let current = active.fetch_add(1, Ordering::SeqCst) + 1;
            maximum.fetch_max(current, Ordering::SeqCst);
            calls.fetch_add(1, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(5));
            active.fetch_sub(1, Ordering::SeqCst);
            Ok(())
        });
        assert!(result.is_ok());
        assert_eq!(calls.load(Ordering::SeqCst), 9);
        assert!((1..=4).contains(&maximum.load(Ordering::SeqCst)));
        assert_eq!(active.load(Ordering::SeqCst), 0);

        for panic_worker in [false, true] {
            let calls = AtomicUsize::new(0);
            let completed = AtomicUsize::new(0);
            let token = CancellationToken::default();
            let result = inspect_images_with(&images, &token, &|image, _| {
                calls.fetch_add(1, Ordering::SeqCst);
                let _completion = Completion(&completed);
                if image == Path::new("image-0") {
                    if panic_worker {
                        panic!("probe worker panic");
                    }
                    Err(Error::InvalidInput("probe failed".into()))
                } else {
                    Ok(())
                }
            });
            assert!(result.is_err(), "panic_worker={panic_worker}");
            assert_eq!(calls.load(Ordering::SeqCst), 4);
            assert_eq!(completed.load(Ordering::SeqCst), 4);
            assert!(!token.is_cancelled());
        }
    }

    #[test]
    fn image_probe_cancellation_joins_started_workers_and_keeps_other_tokens_independent() {
        let images: Vec<_> = (0..9)
            .map(|index| PathBuf::from(format!("image-{index}")))
            .collect();
        let calls = AtomicUsize::new(0);
        let completed = AtomicUsize::new(0);
        let token = CancellationToken::default();
        let result = inspect_images_with(&images, &token, &|_, token| {
            calls.fetch_add(1, Ordering::SeqCst);
            let _completion = Completion(&completed);
            token.cancel();
            token.check()
        });
        assert!(matches!(result, Err(Error::Cancelled)));
        let started = calls.load(Ordering::SeqCst);
        assert!((1..=4).contains(&started));
        assert_eq!(completed.load(Ordering::SeqCst), started);

        let independent = CancellationToken::default();
        let independent_calls = AtomicUsize::new(0);
        let result = inspect_images_with(&images, &independent, &|_, token| {
            independent_calls.fetch_add(1, Ordering::SeqCst);
            token.check()
        });
        assert!(result.is_ok());
        assert_eq!(independent_calls.load(Ordering::SeqCst), images.len());
        assert!(!independent.is_cancelled());
    }
}

#[cfg(test)]
mod linked_fallback_tests {
    use super::*;
    use crate::{Codec, EncoderCapability, RenderSettings};
    fn caps() -> Capabilities {
        Capabilities {
            encoders: vec![
                EncoderCapability {
                    codec: "h264".into(),
                    encoder: "avid_missing_hardware_regression".into(),
                    hardware: true,
                    compiled: true,
                    usable: true,
                    detail: None,
                },
                EncoderCapability {
                    codec: "h264".into(),
                    encoder: "libx264".into(),
                    hardware: false,
                    compiled: true,
                    usable: true,
                    detail: None,
                },
            ],
            ..Default::default()
        }
    }
    #[test]
    fn automatic_fallback_retains_native_errors_and_never_retries_explicit_or_cancelled_jobs() {
        let caps = caps();
        let events = ();
        let token = CancellationToken::default();
        let run = |name: &str| {
            Err(Error::Linked {
                operation: "test native failure",
                code: if name == "libx264" { -12 } else { -22 },
                detail: name.into(),
            })
        };
        let error = encode_with_fallback(
            Codec::H264,
            Encoding::Automatic,
            "avid_missing_hardware_regression",
            &caps,
            &token,
            &events,
            run,
        )
        .unwrap_err();
        assert!(
            matches!(error,Error::Fallback{hardware,software} if matches!(*hardware,Error::Linked{code:-22,..}) && matches!(*software,Error::Linked{code:-12,..}))
        );
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let error = encode_with_fallback(
            Codec::H264,
            Encoding::Hardware,
            "avid_missing_hardware_regression",
            &caps,
            &token,
            &events,
            |_| {
                calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Err(Error::Linked {
                    operation: "hardware",
                    code: -22,
                    detail: "failed".into(),
                })
            },
        )
        .unwrap_err();
        assert!(matches!(error, Error::Linked { .. }));
        assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 1);
        let error = encode_with_fallback(
            Codec::H264,
            Encoding::Automatic,
            "avid_missing_hardware_regression",
            &caps,
            &token,
            &events,
            |_| {
                token.cancel();
                Err(Error::Linked {
                    operation: "hardware",
                    code: -22,
                    detail: "interrupted".into(),
                })
            },
        )
        .unwrap_err();
        assert!(matches!(error, Error::Cancelled));
    }
    #[test]
    fn native_initialization_failure_falls_back_to_a_real_linked_export_when_software_is_compiled()
    {
        use std::io::Write;
        let root = tempfile::tempdir().unwrap();
        let image = root.path().join("art.png");
        let mut png = png::Encoder::new(std::fs::File::create(&image).unwrap(), 32, 16);
        png.set_color(png::ColorType::Rgb);
        png.set_depth(png::BitDepth::Eight);
        png.write_header()
            .unwrap()
            .write_image_data(&[255, 0, 0].repeat(32 * 16))
            .unwrap();
        let audio = root.path().join("audio.wav");
        let rate = 44100u32;
        let bytes = rate * 2 / 10;
        let mut file = std::fs::File::create(&audio).unwrap();
        file.write_all(b"RIFF").unwrap();
        file.write_all(&(36 + bytes).to_le_bytes()).unwrap();
        file.write_all(b"WAVEfmt ").unwrap();
        file.write_all(&16u32.to_le_bytes()).unwrap();
        file.write_all(&1u16.to_le_bytes()).unwrap();
        file.write_all(&1u16.to_le_bytes()).unwrap();
        file.write_all(&rate.to_le_bytes()).unwrap();
        file.write_all(&(rate * 2).to_le_bytes()).unwrap();
        file.write_all(&2u16.to_le_bytes()).unwrap();
        file.write_all(&16u16.to_le_bytes()).unwrap();
        file.write_all(b"data").unwrap();
        file.write_all(&bytes.to_le_bytes()).unwrap();
        file.write_all(&vec![0; bytes as usize]).unwrap();
        drop(file);
        let renderer = Renderer::new();
        let token = CancellationToken::default();
        let mut capabilities = renderer.capabilities(&token).unwrap();
        capabilities.encoders.push(caps().encoders.remove(0));
        let output = root.path().join("fallback.mp4");
        let request = crate::RenderRequest {
            input: Input::Single { image, audio },
            settings: RenderSettings {
                width: 160,
                height: 90,
                fps: 24,
                ..Default::default()
            },
            output: output.clone(),
            protected_paths: vec![],
        };
        let result = encode_with_fallback(
            Codec::H264,
            Encoding::Automatic,
            "avid_missing_hardware_regression",
            &capabilities,
            &token,
            &(),
            |name| {
                let plan =
                    command::linked_plan(&request, name, &output, RenderMode::PerFrame, Some(0.1))?;
                renderer.linked(
                    &token,
                    Some(Duration::from_secs(20)),
                    "encode fallback regression",
                    Some(0.1),
                    &(),
                    |control| avid_media_bridge::execute(&plan, control),
                )
            },
        );
        if capabilities
            .encoders
            .iter()
            .any(|e| e.encoder == "libx264" && e.usable)
        {
            result.unwrap();
            assert_eq!(renderer.inspect_image(&output, &token).unwrap(), (160, 90));
            assert!(
                renderer
                    .probe_audio_duration(&output, &token)
                    .unwrap()
                    .unwrap()
                    > 0.05
            );
        } else {
            assert!(matches!(result, Err(Error::Fallback { .. })));
        }
    }
}
