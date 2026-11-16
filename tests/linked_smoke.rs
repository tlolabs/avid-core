//! One small native linked export for the weekly diagnostic smoke path.
use avid_core::*;
use std::{
    io::Write,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

// Keep the scheduled smoke self-contained. Binary fixture coverage lives in the
// deliberate decoding suite, and source-only review copies can run this smoke.
fn sources(root: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let image = root.join("cover.png");
    let mut png = png::Encoder::new(std::fs::File::create(&image).unwrap(), 32, 16);
    png.set_color(png::ColorType::Rgb);
    png.set_depth(png::BitDepth::Eight);
    let mut writer = png.write_header().unwrap();
    writer
        .write_image_data(&[255, 0, 0].repeat(32 * 16))
        .unwrap();
    writer.finish().unwrap();
    let audio = root.join("audio.wav");
    let rate = 44100u32;
    let sample_count = rate / 5;
    let size = sample_count * 2;
    let mut file = std::fs::File::create(&audio).unwrap();
    file.write_all(b"RIFF").unwrap();
    file.write_all(&(36 + size).to_le_bytes()).unwrap();
    file.write_all(b"WAVEfmt ").unwrap();
    file.write_all(&16u32.to_le_bytes()).unwrap();
    file.write_all(&1u16.to_le_bytes()).unwrap();
    file.write_all(&1u16.to_le_bytes()).unwrap();
    file.write_all(&rate.to_le_bytes()).unwrap();
    file.write_all(&(rate * 2).to_le_bytes()).unwrap();
    file.write_all(&2u16.to_le_bytes()).unwrap();
    file.write_all(&16u16.to_le_bytes()).unwrap();
    file.write_all(b"data").unwrap();
    file.write_all(&size.to_le_bytes()).unwrap();
    for index in 0..sample_count {
        let sample =
            (12000.0 * (index as f64 * 440.0 * std::f64::consts::TAU / rate as f64).sin()) as i16;
        file.write_all(&sample.to_le_bytes()).unwrap();
    }
    (image, audio)
}

#[test]
fn native_linked_probe_preview_and_export() {
    let renderer = Renderer::new();
    let directory = tempfile::tempdir().unwrap();
    let (image, audio) = sources(directory.path());
    let token = CancellationToken::default();
    assert!(
        renderer
            .probe_audio_duration(&audio, &token)
            .unwrap()
            .unwrap()
            > 0.0
    );
    let settings = RenderSettings {
        width: 160,
        height: 90,
        fps: 12,
        ..renderer.default_settings()
    };
    let preview = directory.path().join("preview.png");
    renderer
        .preview(
            &PreviewRequest {
                image: image.clone(),
                output: preview.clone(),
                settings: settings.clone(),
                protected_paths: vec![],
            },
            &token,
            &(),
        )
        .unwrap();
    assert_eq!(renderer.inspect_image(&preview, &token).unwrap(), (160, 90));
    #[derive(Default)]
    struct Events {
        progress: AtomicBool,
        complete: AtomicBool,
    }
    impl EventSink for Events {
        fn progress(&self, _: Progress) {
            self.progress.store(true, Ordering::Relaxed);
        }
        fn stage(&self, stage: Stage) {
            if stage == Stage::Complete {
                self.complete.store(true, Ordering::Relaxed);
            }
        }
    }
    let events = Events::default();
    let output = directory.path().join("smoke.mp4");
    renderer
        .render_with_mode(
            &RenderRequest {
                input: Input::Single { image, audio },
                settings,
                output: output.clone(),
                protected_paths: vec![],
            },
            RenderMode::Simple,
            &token,
            &events,
        )
        .unwrap();
    assert!(std::fs::metadata(&output).unwrap().len() > 128);
    assert_eq!(renderer.inspect_image(&output, &token).unwrap(), (160, 90));
    assert!(
        renderer
            .probe_audio_duration(&output, &token)
            .unwrap()
            .unwrap()
            > 0.0
    );
    assert!(events.progress.load(Ordering::Relaxed));
    assert!(events.complete.load(Ordering::Relaxed));
}
