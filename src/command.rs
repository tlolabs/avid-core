use crate::{
    Capabilities, Codec, Composition, EncoderCapability, Encoding, Error, Input, RenderRequest,
    RenderSettings, Result,
};
use std::{
    ffi::{OsStr, OsString},
    path::Path,
};
pub(crate) fn os(v: impl AsRef<OsStr>) -> OsString {
    v.as_ref().to_owned()
}
fn strings(v: &[&str]) -> Vec<OsString> {
    v.iter().map(os).collect()
}
pub(crate) fn artwork_graph(
    input: &str,
    output: &str,
    suffix: &str,
    settings: &RenderSettings,
    tail: &str,
) -> String {
    let (w, h) = (settings.width, settings.height);
    let square = w.min(h);
    let flips = match (settings.flip_horizontal, settings.flip_vertical) {
        (true, true) => "hflip,vflip,",
        (true, false) => "hflip,",
        (false, true) => "vflip,",
        _ => "",
    };
    let (blur, pad) = match settings.composition {
        Composition::Fitted => (40, String::new()),
        Composition::SquarePadded => (20, format!(",pad={square}:{square}:(ow-iw)/2:(oh-ih)/2")),
    };
    format!("[{input}]{flips}split=2[bgsrc{suffix}][fgsrc{suffix}];[bgsrc{suffix}]scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h},gblur=sigma={blur}[bg{suffix}];[fgsrc{suffix}]scale={square}:{square}:force_original_aspect_ratio=decrease{pad}[fg{suffix}];[bg{suffix}][fg{suffix}]overlay=(W-w)/2:(H-h)/2{tail},format=yuv420p[{output}]")
}
pub(crate) fn preview(image: &Path, output: &Path, settings: &RenderSettings) -> Vec<OsString> {
    let mut args = strings(&[
        "-nostdin",
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-protocol_whitelist",
        "file,pipe",
        "-i",
    ]);
    args.push(os(image));
    args.extend([
        os("-filter_complex"),
        os(artwork_graph("0:v", "video", "", settings, "")),
    ]);
    args.extend(strings(&[
        "-map",
        "[video]",
        "-frames:v",
        "1",
        "-f",
        "image2",
    ]));
    args.push(os(output));
    args
}
pub(crate) fn export(
    request: &RenderRequest,
    encoder: &str,
    output: &Path,
) -> Result<Vec<OsString>> {
    request.settings.validate()?;
    let s = &request.settings;
    let mut args = strings(&["-nostdin", "-hide_banner", "-loglevel", "warning", "-y"]);
    match &request.input {
        Input::Single { image, audio } => {
            args.extend([
                os("-loop"),
                os("1"),
                os("-framerate"),
                os(s.fps.to_string()),
                os("-protocol_whitelist"),
                os("file,pipe"),
                os("-i"),
                os(image),
                os("-protocol_whitelist"),
                os("file,pipe"),
                os("-i"),
                os(audio),
            ]);
            args.extend([
                os("-filter_complex"),
                os(artwork_graph("0:v", "video", "", s, "")),
            ]);
            args.extend(strings(&["-map", "[video]", "-map", "1:a:0"]));
        }
        Input::Timeline(timeline) => {
            if timeline.clips().is_empty() {
                return Err(Error::InvalidInput(
                    "Select at least one clip for video export".into(),
                ));
            }
            let mut graph = String::new();
            for (i, clip) in timeline.clips().iter().enumerate() {
                let duration = format!("{:.6}", clip.duration_seconds);
                args.extend([
                    os("-loop"),
                    os("1"),
                    os("-framerate"),
                    os(s.fps.to_string()),
                    os("-t"),
                    os(&duration),
                    os("-protocol_whitelist"),
                    os("file,pipe"),
                    os("-i"),
                    os(&clip.image),
                    os("-t"),
                    os(&duration),
                    os("-protocol_whitelist"),
                    os("file,pipe"),
                    os("-i"),
                    os(&clip.audio),
                ]);
                graph.push_str(&artwork_graph(
                    &format!("{}:v", i * 2),
                    &format!("v{i}"),
                    &i.to_string(),
                    s,
                    &format!(",trim=duration={duration},setpts=PTS-STARTPTS"),
                ));
                graph.push_str(&format!(";[{}:a:0]atrim=duration={duration},aformat=sample_rates=48000:channel_layouts=stereo,asetpts=PTS-STARTPTS[a{i}];",i*2+1));
            }
            for i in 0..timeline.clips().len() {
                graph.push_str(&format!("[v{i}][a{i}]"));
            }
            graph.push_str(&format!(
                "concat=n={}:v=1:a=1[outv][outa]",
                timeline.clips().len()
            ));
            args.extend([os("-filter_complex"), os(graph)]);
            args.extend(strings(&["-map", "[outv]", "-map", "[outa]"]));
        }
    }
    args.extend([os("-c:v"), os(encoder)]);
    if encoder == "libx264" {
        args.extend(strings(&["-tune", "stillimage"]));
    }
    if s.codec == Codec::Hevc {
        args.extend(strings(&["-tag:v", "hvc1"]));
    }
    args.extend(strings(&["-pix_fmt", "yuv420p"]));
    if matches!(request.input, Input::Timeline(_)) {
        args.extend([os("-r"), os(s.fps.to_string())]);
    }
    args.extend([os("-c:a"), os("aac"), os("-b:a"), os(&s.audio_bitrate)]);
    args.extend(strings(&[
        "-shortest",
        "-movflags",
        "+faststart",
        "-f",
        "mp4",
        "-progress",
        "pipe:1",
        "-nostats",
    ]));
    args.push(os(output));
    Ok(args)
}
fn encoders(codec: Codec) -> (&'static str, &'static [&'static str]) {
    match codec {
        Codec::H264 => (
            "libx264",
            &[
                "h264_videotoolbox",
                "h264_nvenc",
                "h264_qsv",
                "h264_amf",
                "h264_vaapi",
            ],
        ),
        Codec::Hevc => (
            "libx265",
            &[
                "hevc_videotoolbox",
                "hevc_nvenc",
                "hevc_qsv",
                "hevc_amf",
                "hevc_vaapi",
            ],
        ),
    }
}
pub(crate) fn parse_capabilities(text: &str) -> Capabilities {
    let names: Vec<_> = text
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let flags = fields.next()?;
            let name = fields.next()?;
            (flags.starts_with('V') && flags.len() == 6).then_some(name)
        })
        .collect();
    let mut result = Capabilities::default();
    for codec in [Codec::H264, Codec::Hevc] {
        let (software, hardware) = encoders(codec);
        for encoder in std::iter::once(software).chain(hardware.iter().copied()) {
            if names.contains(&encoder) {
                result.encoders.push(EncoderCapability {
                    codec: if codec == Codec::H264 { "h264" } else { "hevc" }.into(),
                    encoder: encoder.into(),
                    hardware: encoder != software,
                });
            }
        }
    }
    result
}
pub(crate) fn select_encoder(
    codec: Codec,
    mode: Encoding,
    capabilities: &Capabilities,
) -> Result<String> {
    let (software, hardware) = encoders(codec);
    let available = |name: &str| capabilities.encoders.iter().any(|e| e.encoder == name);
    if mode != Encoding::Software {
        if let Some(name) = hardware.iter().find(|name| available(name)) {
            return Ok((*name).to_owned());
        }
        if mode == Encoding::Hardware {
            return Err(Error::InvalidInput(format!(
                "Hardware {codec:?} requested, but no compatible encoder is advertised"
            )));
        }
    }
    if available(software) {
        Ok(software.into())
    } else {
        Err(Error::InvalidInput(format!(
            "Required software encoder {software} is unavailable"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Clip, Timeline};
    fn request(input: Input) -> RenderRequest {
        RenderRequest {
            input,
            settings: RenderSettings::default(),
            output: "out.mp4".into(),
            protected_paths: vec![],
        }
    }
    fn joined(args: &[OsString]) -> String {
        args.iter()
            .map(|v| v.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ")
    }
    #[test]
    fn single_graph_matches_ativ_reference() {
        let s = RenderSettings {
            width: 1080,
            height: 1920,
            flip_horizontal: true,
            flip_vertical: true,
            ..Default::default()
        };
        assert_eq!(artwork_graph("0:v","video","",&s,""),"[0:v]hflip,vflip,split=2[bgsrc][fgsrc];[bgsrc]scale=1080:1920:force_original_aspect_ratio=increase,crop=1080:1920,gblur=sigma=40[bg];[fgsrc]scale=1080:1080:force_original_aspect_ratio=decrease[fg];[bg][fg]overlay=(W-w)/2:(H-h)/2,format=yuv420p[video]");
    }
    #[test]
    fn single_command_preserves_audio_layout_and_arguments() {
        let r = request(Input::Single {
            image: "art ü with spaces.png".into(),
            audio: "-track.wav".into(),
        });
        let args = export(&r, "libx264", Path::new("out.mp4")).unwrap();
        assert!(args.contains(&os("art ü with spaces.png")));
        let s = joined(&args);
        for expected in [
            "-c:v libx264 -tune stillimage",
            "-pix_fmt yuv420p",
            "-c:a aac -b:a 128k",
            "-shortest",
            "-movflags +faststart",
            "-progress pipe:1",
            "-map 1:a:0",
        ] {
            assert!(s.contains(expected), "{s}");
        }
        for absent in [
            "atrim",
            "concat=",
            "aformat",
            "-map_metadata",
            "-map_chapters",
            "-r 30",
        ] {
            assert!(!s.contains(absent));
        }
    }
    #[test]
    fn sequence_preserves_encap_normalization_cuts_and_hevc() {
        let timeline = Timeline::new(vec![
            Clip {
                id: "one".into(),
                image: "art.png".into(),
                audio: "one.wav".into(),
                duration_seconds: 2.0,
            },
            Clip {
                id: "two".into(),
                image: "art2.png".into(),
                audio: "two.wav".into(),
                duration_seconds: 3.0,
            },
        ])
        .unwrap();
        let mut r = request(Input::Timeline(timeline));
        r.settings.composition = Composition::SquarePadded;
        r.settings.codec = Codec::Hevc;
        let s = joined(&export(&r, "libx265", Path::new("out.mp4")).unwrap());
        for expected in [
            "gblur=sigma=20",
            "pad=1080:1080",
            "[2:v]split",
            "[3:a:0]atrim=duration=3.000000",
            "aformat=sample_rates=48000:channel_layouts=stereo",
            "[v0][a0][v1][a1]concat=n=2:v=1:a=1",
            "-tag:v hvc1",
            "-r 30",
        ] {
            assert!(s.contains(expected), "{s}");
        }
        for absent in ["mp3", "crossfade", "subtitles", "-map_metadata"] {
            assert!(!s.contains(absent));
        }
    }
    #[test]
    fn encoder_detection_does_not_match_descriptions() {
        let c=parse_capabilities(" V..... libx264 H264\n V..... h264_videotoolbox hardware\n A..... aac mentions hevc_nvenc\n V..... unrelated mentions libx265");
        assert_eq!(c.encoders.len(), 2);
        assert_eq!(
            select_encoder(Codec::H264, Encoding::Automatic, &c).unwrap(),
            "h264_videotoolbox"
        );
        assert_eq!(
            select_encoder(Codec::H264, Encoding::Software, &c).unwrap(),
            "libx264"
        );
        assert!(select_encoder(Codec::Hevc, Encoding::Hardware, &c).is_err());
        assert!(select_encoder(Codec::Hevc, Encoding::Software, &c).is_err());
    }
}

#[cfg(test)]
mod reference_tests {
    use super::*;
    use crate::{Clip, Timeline};
    #[test]
    fn full_sequence_graph_matches_executed_encap_reference() {
        let clips = (0..2)
            .map(|i| Clip {
                id: i.to_string(),
                image: format!("art{i}.png").into(),
                audio: format!("audio{i}.wav").into(),
                duration_seconds: 2.0 + f64::from(i),
            })
            .collect();
        let request = RenderRequest {
            input: Input::Timeline(Timeline::new(clips).unwrap()),
            settings: RenderSettings {
                width: 160,
                height: 90,
                flip_horizontal: true,
                composition: Composition::SquarePadded,
                ..Default::default()
            },
            output: "out.mp4".into(),
            protected_paths: vec![],
        };
        let args = export(&request, "libx264", Path::new("out.mp4")).unwrap();
        let graph = &args[args.iter().position(|a| a == "-filter_complex").unwrap() + 1];
        assert_eq!(
            graph.to_str().unwrap(),
            include_str!("../tests/fixtures/encap-graph.txt").trim()
        );
    }
}
