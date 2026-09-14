//! Deterministic process tests. Fake executable scripts require a POSIX shell, not FFmpeg.
#![cfg(unix)]
use avid_core::*;
use std::{fs, os::unix::fs::PermissionsExt, path::Path, sync::Mutex, time::Duration};
fn script(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}
fn renderer(root: &Path, mode: &str) -> Renderer {
    let ffmpeg = root.join("ffmpeg");
    let ffprobe = root.join("ffprobe");
    script(&ffprobe,"#!/bin/sh\nif [ \"$1\" = '-version' ]; then echo 'ffprobe version test'; exit; fi\nfor arg in \"$@\"; do if [ \"$arg\" = 'stream=width,height' ]; then echo 64x48; exit; fi; done\necho 2\n");
    let body = format!(
        r#"#!/bin/sh
if [ "$1" = '-version' ]; then echo 'ffmpeg version test'; exit; fi
if [ "$2" = '-encoders' ]; then printf ' V..... libx264 software\n V..... h264_videotoolbox hardware\n'; exit; fi
encoder=''
previous=''
for arg in "$@"; do
  if [ "$previous" = '-c:v' ]; then encoder="$arg"; fi
  previous="$arg"
  output="$arg"
done
printf partial > "$output"
printf 'out_time_us=1000000\nspeed=2x\nprogress=continue\n'
if [ '{mode}' = 'slow' ]; then exec sleep 10; fi
if [ '{mode}' = 'fail' ] || [ "$encoder" = 'h264_videotoolbox' ]; then echo 'deliberate encoder failure' >&2; exit 7; fi
printf completed > "$output"
printf 'out_time_us=2000000\nspeed=2x\nprogress=end\n'
"#
    );
    script(&ffmpeg, &body);
    let tools = MediaTools::discover(
        ToolDiscovery {
            ffmpeg: Some(ffmpeg),
            ffprobe: Some(ffprobe),
            search_path: false,
            ..Default::default()
        },
        &CancellationToken::default(),
    )
    .unwrap();
    Renderer::new(tools)
}
fn request(root: &Path) -> RenderRequest {
    let image = root.join("art ü.png");
    let audio = root.join("source audio.wav");
    fs::write(&image, b"art").unwrap();
    fs::write(&audio, b"audio").unwrap();
    let output = root.join("result.mp4");
    fs::write(&output, b"old output").unwrap();
    RenderRequest {
        input: Input::Single { image, audio },
        settings: RenderSettings::default(),
        output,
        protected_paths: vec![],
    }
}
fn no_stages(root: &Path) {
    assert!(!fs::read_dir(root).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".avid-")));
}
#[derive(Default)]
struct Events {
    stages: Mutex<Vec<Stage>>,
    progress: Mutex<Vec<Progress>>,
}
impl EventSink for Events {
    fn stage(&self, stage: Stage) {
        self.stages.lock().unwrap().push(stage);
    }
    fn progress(&self, p: Progress) {
        self.progress.lock().unwrap().push(p);
    }
}
#[test]
fn automatic_fallback_and_progress_share_a_guarded_stage() {
    let root = tempfile::tempdir().unwrap();
    let renderer = renderer(root.path(), "ok");
    let mut request = request(root.path());
    request.settings.encoding = Encoding::Automatic;
    let events = Events::default();
    renderer
        .render(&request, &CancellationToken::default(), &events)
        .unwrap();
    assert_eq!(fs::read(&request.output).unwrap(), b"completed");
    no_stages(root.path());
    let stages = events.stages.lock().unwrap();
    assert_eq!(stages.iter().filter(|s| **s == Stage::Encoding).count(), 2);
    assert_eq!(stages.last(), Some(&Stage::Complete));
    assert!(events
        .progress
        .lock()
        .unwrap()
        .iter()
        .any(|p| p.fraction == Some(1.0)));
}
#[test]
fn failed_fallback_retains_both_errors_and_old_output() {
    let root = tempfile::tempdir().unwrap();
    let renderer = renderer(root.path(), "fail");
    let mut request = request(root.path());
    request.settings.encoding = Encoding::Automatic;
    match renderer
        .render(&request, &CancellationToken::default(), &())
        .unwrap_err()
    {
        Error::Fallback { hardware, software } => {
            assert!(hardware.to_string().contains("deliberate encoder failure"));
            assert!(software.to_string().contains("deliberate encoder failure"));
        }
        e => panic!("{e:?}"),
    }
    assert_eq!(fs::read(request.output).unwrap(), b"old output");
    no_stages(root.path());
}
#[test]
fn explicit_hardware_failure_does_not_fall_back() {
    let root = tempfile::tempdir().unwrap();
    let renderer = renderer(root.path(), "ok");
    let mut request = request(root.path());
    request.settings.encoding = Encoding::Hardware;
    assert!(matches!(
        renderer.render(&request, &CancellationToken::default(), &()),
        Err(Error::Process { .. })
    ));
    assert_eq!(fs::read(request.output).unwrap(), b"old output");
    no_stages(root.path());
}
#[test]
fn render_timeout_preserves_destination_and_removes_partial() {
    let root = tempfile::tempdir().unwrap();
    let renderer = renderer(root.path(), "slow").with_options(OperationOptions {
        render_timeout: Some(Duration::from_millis(80)),
        ..Default::default()
    });
    let request = request(root.path());
    assert!(matches!(
        renderer.render(&request, &CancellationToken::default(), &()),
        Err(Error::Timeout(_))
    ));
    assert_eq!(fs::read(request.output).unwrap(), b"old output");
    no_stages(root.path());
}
struct CancelAtPublication(CancellationToken);
impl EventSink for CancelAtPublication {
    fn stage(&self, stage: Stage) {
        if stage == Stage::Publishing {
            self.0.cancel();
        }
    }
}
#[test]
fn cancellation_after_encoding_prevents_publication() {
    let root = tempfile::tempdir().unwrap();
    let renderer = renderer(root.path(), "ok");
    let request = request(root.path());
    let token = CancellationToken::default();
    assert!(matches!(
        renderer.render(&request, &token, &CancelAtPublication(token.clone())),
        Err(Error::Cancelled)
    ));
    assert_eq!(fs::read(request.output).unwrap(), b"old output");
    no_stages(root.path());
}
#[test]
fn cancellation_during_encode_reaps_and_cleans() {
    let root = tempfile::tempdir().unwrap();
    let renderer = renderer(root.path(), "slow");
    let request = request(root.path());
    let output = request.output.clone();
    let token = CancellationToken::default();
    let worker_token = token.clone();
    let worker = std::thread::spawn(move || renderer.render(&request, &worker_token, &()));
    std::thread::sleep(Duration::from_millis(200));
    token.cancel();
    assert!(matches!(worker.join().unwrap(), Err(Error::Cancelled)));
    assert_eq!(fs::read(output).unwrap(), b"old output");
    no_stages(root.path());
}
#[test]
fn preview_protects_original_and_cleans_failed_output() {
    let root = tempfile::tempdir().unwrap();
    let renderer = renderer(root.path(), "fail");
    let request = request(root.path());
    let image = match request.input {
        Input::Single { image, .. } => image,
        _ => unreachable!(),
    };
    let mut preview = PreviewRequest {
        image: image.clone(),
        output: image.clone(),
        settings: RenderSettings::default(),
        protected_paths: vec![],
    };
    assert!(matches!(
        renderer.preview(&preview, &CancellationToken::default(), &()),
        Err(Error::InvalidInput(_))
    ));
    assert_eq!(fs::read(&image).unwrap(), b"art");
    preview.output = root.path().join("preview.png");
    fs::write(&preview.output, b"old preview").unwrap();
    assert!(renderer
        .preview(&preview, &CancellationToken::default(), &())
        .is_err());
    assert_eq!(fs::read(&preview.output).unwrap(), b"old preview");
    no_stages(root.path());
}
#[test]
fn extra_protected_paths_cover_unselected_sources_and_project() {
    let root = tempfile::tempdir().unwrap();
    let renderer = renderer(root.path(), "ok");
    let mut request = request(root.path());
    request.protected_paths.push(request.output.clone());
    assert!(matches!(
        renderer.render(&request, &CancellationToken::default(), &()),
        Err(Error::InvalidInput(_))
    ));
    assert_eq!(fs::read(request.output).unwrap(), b"old output");
}
#[test]
fn probe_and_preview_timeouts_are_cancellable() {
    let root = tempfile::tempdir().unwrap();
    let renderer = renderer(root.path(), "slow").with_options(OperationOptions {
        probe_timeout: Some(Duration::from_millis(50)),
        preview_timeout: Some(Duration::from_millis(50)),
        ..Default::default()
    });
    let request = request(root.path());
    let Input::Single { image, audio } = request.input else {
        unreachable!()
    };
    let preview = PreviewRequest {
        image,
        output: root.path().join("preview.png"),
        settings: RenderSettings::default(),
        protected_paths: vec![],
    };
    assert!(matches!(
        renderer.preview(&preview, &CancellationToken::default(), &()),
        Err(Error::Timeout(_))
    ));
    no_stages(root.path());
    script(renderer.tools().ffprobe(), "#!/bin/sh\nexec sleep 10\n");
    assert!(matches!(
        renderer.probe_audio_duration(&audio, &CancellationToken::default()),
        Err(Error::Timeout(_))
    ));
}

#[test]
fn concurrent_operations_have_independent_cancellation_and_staging() {
    let root = tempfile::tempdir().unwrap();
    let renderer = renderer(root.path(), "ok");
    let first = request(root.path());
    let mut second = first.clone();
    second.output = root.path().join("second.mp4");
    let first_output = first.output.clone();
    let second_output = second.output.clone();
    let other = renderer.clone();
    let a = std::thread::spawn(move || {
        let token = CancellationToken::default();
        other.render(&first, &token, &CancelAtPublication(token.clone()))
    });
    let b =
        std::thread::spawn(move || renderer.render(&second, &CancellationToken::default(), &()));
    assert!(matches!(a.join().unwrap(), Err(Error::Cancelled)));
    b.join().unwrap().unwrap();
    assert_eq!(fs::read(first_output).unwrap(), b"old output");
    assert_eq!(fs::read(second_output).unwrap(), b"completed");
    no_stages(root.path());
}
