//! Explicit opt-in integration tests: cargo test --test ffmpeg -- --ignored
use avid_core::*;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
};
fn renderer() -> Renderer {
    Renderer::new(
        MediaTools::discover(ToolDiscovery::default(), &CancellationToken::default())
            .expect("Install FFmpeg/ffprobe with libx264, libx265, AAC and image filters"),
    )
}
fn run(program: &Path, args: &[&str]) {
    let out = Command::new(program).args(args).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
fn sources(renderer: &Renderer, root: &Path, color: &str, frequency: &str) -> (PathBuf, PathBuf) {
    let art = root.join(format!("artwork ü {color}.png"));
    let audio = root.join(format!("track {frequency}.wav"));
    run(
        renderer.tools().ffmpeg(),
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            &format!("color=c={color}:s=96x64"),
            "-frames:v",
            "1",
            art.to_str().unwrap(),
        ],
    );
    run(
        renderer.tools().ffmpeg(),
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            &format!("sine=frequency={frequency}:duration=1:sample_rate=44100"),
            "-c:a",
            "pcm_s16le",
            audio.to_str().unwrap(),
        ],
    );
    (art, audio)
}
fn inspect(renderer: &Renderer, path: &Path) -> Value {
    let output = Command::new(renderer.tools().ffprobe())
        .args([
            "-v",
            "error",
            "-show_streams",
            "-show_format",
            "-show_chapters",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .unwrap();
    assert!(output.status.success());
    serde_json::from_slice(&output.stdout).unwrap()
}
fn streams(info: &Value) -> (&Value, &Value) {
    let streams = info["streams"].as_array().unwrap();
    (
        streams.iter().find(|s| s["codec_type"] == "video").unwrap(),
        streams.iter().find(|s| s["codec_type"] == "audio").unwrap(),
    )
}
#[derive(Default)]
struct Events(Mutex<Vec<Progress>>);
impl EventSink for Events {
    fn progress(&self, p: Progress) {
        self.0.lock().unwrap().push(p);
    }
}
#[test]
#[ignore = "requires FFmpeg and ffprobe"]
fn standalone_preview_render_and_original_audio_format() {
    let renderer = renderer();
    let root = tempfile::Builder::new()
        .prefix("avid integration ü ")
        .tempdir()
        .unwrap();
    let (image, audio) = sources(&renderer, root.path(), "red", "523");
    let token = CancellationToken::default();
    let duration = renderer
        .probe_audio_duration(&audio, &token)
        .unwrap()
        .unwrap();
    assert!((duration - 1.0).abs() < 0.01);
    let settings = RenderSettings {
        width: 160,
        height: 90,
        fps: 24,
        flip_horizontal: true,
        ..Default::default()
    };
    let preview = PreviewRequest {
        image: image.clone(),
        output: root.path().join("preview image.png"),
        settings: settings.clone(),
        protected_paths: vec![],
    };
    renderer.preview(&preview, &token, &()).unwrap();
    assert_eq!(
        renderer.inspect_image(&preview.output, &token).unwrap(),
        (160, 90)
    );
    let output = root.path().join("finished video.mp4");
    let events = Events::default();
    renderer
        .render(
            &RenderRequest {
                input: Input::Single { image, audio },
                settings,
                output: output.clone(),
                protected_paths: vec![],
            },
            &token,
            &events,
        )
        .unwrap();
    let info = inspect(&renderer, &output);
    let (v, a) = streams(&info);
    assert_eq!(v["codec_name"], "h264");
    assert_eq!(v["pix_fmt"], "yuv420p");
    assert_eq!(v["width"], 160);
    assert_eq!(v["height"], 90);
    assert_eq!(a["codec_name"], "aac");
    assert_eq!(a["sample_rate"], "44100");
    assert_eq!(a["channels"], 1);
    assert!(!events.0.lock().unwrap().is_empty());
    assert!(!fs::read_dir(root.path()).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".avid-")));
}
#[test]
#[ignore = "requires FFmpeg and ffprobe"]
fn sequence_hard_cuts_normalizes_audio_and_preserves_order() {
    let renderer = renderer();
    let root = tempfile::tempdir().unwrap();
    let (red, a) = sources(&renderer, root.path(), "red", "440");
    let (blue, b) = sources(&renderer, root.path(), "blue", "880");
    let clips = vec![
        Clip {
            id: "red".into(),
            image: red,
            audio: a,
            duration_seconds: 1.0,
        },
        Clip {
            id: "blue".into(),
            image: blue,
            audio: b,
            duration_seconds: 1.0,
        },
    ];
    let timeline = Timeline::select(&clips, &["blue".into(), "red".into()], true).unwrap();
    let output = root.path().join("sequence.mp4");
    renderer
        .render(
            &RenderRequest {
                input: Input::Timeline(timeline),
                settings: RenderSettings {
                    width: 160,
                    height: 90,
                    composition: Composition::SquarePadded,
                    ..Default::default()
                },
                output: output.clone(),
                protected_paths: vec![],
            },
            &CancellationToken::default(),
            &(),
        )
        .unwrap();
    let info = inspect(&renderer, &output);
    let (_, audio) = streams(&info);
    assert_eq!(audio["sample_rate"], "48000");
    assert_eq!(audio["channels"], 2);
    assert!(info["chapters"].as_array().unwrap().is_empty());
    let duration = info["format"]["duration"]
        .as_str()
        .unwrap()
        .parse::<f64>()
        .unwrap();
    assert!((duration - 2.0).abs() < 0.15, "{duration}");
    for (time, blue) in [("0.3", true), ("1.3", false)] {
        let out = Command::new(renderer.tools().ffmpeg())
            .args(["-v", "error", "-ss", time, "-i"])
            .arg(&output)
            .args([
                "-frames:v",
                "1",
                "-vf",
                "scale=1:1",
                "-pix_fmt",
                "rgb24",
                "-f",
                "rawvideo",
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(out.status.success());
        assert_eq!(out.stdout.len(), 3);
        if blue {
            assert!(out.stdout[2] > out.stdout[0] + 50);
        } else {
            assert!(out.stdout[0] > out.stdout[2] + 50);
        }
    }
}
#[test]
#[ignore = "requires FFmpeg with libx265 and ffprobe"]
fn hevc_has_hvc1_tag() {
    let renderer = renderer();
    let root = tempfile::tempdir().unwrap();
    let (image, audio) = sources(&renderer, root.path(), "green", "440");
    let output = root.path().join("hevc.mp4");
    renderer
        .render(
            &RenderRequest {
                input: Input::Single { image, audio },
                settings: RenderSettings {
                    width: 160,
                    height: 90,
                    fps: 10,
                    codec: Codec::Hevc,
                    ..Default::default()
                },
                output: output.clone(),
                protected_paths: vec![],
            },
            &CancellationToken::default(),
            &(),
        )
        .unwrap();
    let info = inspect(&renderer, &output);
    let (v, _) = streams(&info);
    assert_eq!(v["codec_name"], "hevc");
    assert_eq!(v["codec_tag_string"], "hvc1");
}
