//! Native linked tests use synthetic source bytes and never invoke a media executable.
use avid_core::*;
use std::{
    fs,
    io::Write,
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
    time::Duration,
};
fn inputs(root: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let image = root.join("artwork ü.png");
    let file = fs::File::create(&image).unwrap();
    let mut e = png::Encoder::new(file, 96, 64);
    e.set_color(png::ColorType::Rgb);
    e.set_depth(png::BitDepth::Eight);
    let mut w = e.write_header().unwrap();
    let pixels: Vec<_> = (0..96 * 64)
        .flat_map(|n| {
            if n % 96 < 48 {
                [255, 0, 0]
            } else {
                [0, 0, 255]
            }
        })
        .collect();
    w.write_image_data(&pixels).unwrap();
    drop(w);
    let audio = root.join("initial silence ü.wav");
    let rate = 44100u32;
    let samples: Vec<i16> = (0..rate)
        .map(|i| {
            if i < rate / 4 {
                0
            } else {
                ((i as f64 * 440.0 * std::f64::consts::TAU / rate as f64).sin() * 12000.0) as i16
            }
        })
        .collect();
    let size = (samples.len() * 2) as u32;
    let mut f = fs::File::create(&audio).unwrap();
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + size).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&rate.to_le_bytes()).unwrap();
    f.write_all(&(rate * 2).to_le_bytes()).unwrap();
    f.write_all(&2u16.to_le_bytes()).unwrap();
    f.write_all(&16u16.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&size.to_le_bytes()).unwrap();
    for s in samples {
        f.write_all(&s.to_le_bytes()).unwrap();
    }
    (image, audio)
}
fn settings() -> RenderSettings {
    RenderSettings {
        width: 160,
        height: 90,
        fps: 24,
        encoding: Encoding::Automatic,
        ..Default::default()
    }
}
#[derive(Default)]
struct Events {
    stages: Mutex<Vec<Stage>>,
    progress: AtomicUsize,
}
impl EventSink for Events {
    fn stage(&self, s: Stage) {
        self.stages.lock().unwrap().push(s);
    }
    fn progress(&self, _: Progress) {
        self.progress.fetch_add(1, Ordering::Relaxed);
    }
}
#[test]
fn linked_probe_preview_export_and_timelines() {
    let r = Renderer::new();
    let dir = tempfile::tempdir().unwrap();
    let (image, audio) = inputs(dir.path());
    let token = CancellationToken::default();
    assert_eq!(r.inspect_image(&image, &token).unwrap(), (96, 64));
    assert!((r.probe_audio_duration(&audio, &token).unwrap().unwrap() - 1.0).abs() < 0.01);
    let caps = r.capabilities(&token).unwrap();
    assert!(caps.encoders.iter().any(|e| e.codec == "h264"));
    for composition in [Composition::Fitted, Composition::SquarePadded] {
        let s = RenderSettings {
            composition,
            ..settings()
        };
        let out = dir.path().join("preview.png");
        r.preview(
            &PreviewRequest {
                image: image.clone(),
                output: out.clone(),
                settings: s.clone(),
                protected_paths: vec![],
            },
            &token,
            &(),
        )
        .unwrap();
        assert_eq!(r.inspect_image(&out, &token).unwrap(), (160, 90));
        for mode in [RenderMode::Simple, RenderMode::PerFrame] {
            for timeline in [false, true] {
                let events = Events::default();
                let output = dir.path().join("export ü.mp4");
                let input = if timeline {
                    Input::Timeline(
                        Timeline::new(vec![
                            Clip {
                                id: "one".into(),
                                image: image.clone(),
                                audio: audio.clone(),
                                duration_seconds: 0.5,
                            },
                            Clip {
                                id: "two".into(),
                                image: image.clone(),
                                audio: audio.clone(),
                                duration_seconds: 0.5,
                            },
                        ])
                        .unwrap(),
                    )
                } else {
                    Input::Single {
                        image: image.clone(),
                        audio: audio.clone(),
                    }
                };
                r.render_with_mode(
                    &RenderRequest {
                        input,
                        settings: s.clone(),
                        output: output.clone(),
                        protected_paths: vec![],
                    },
                    mode,
                    &token,
                    &events,
                )
                .unwrap();
                assert!(fs::metadata(&output).unwrap().len() > 100);
                assert_eq!(r.inspect_image(&output, &token).unwrap(), (160, 90));
                let d = r.probe_audio_duration(&output, &token).unwrap().unwrap();
                assert!((d - 1.0).abs() < 0.12, "duration={d}");
                assert!(events.progress.load(Ordering::Relaxed) > 0);
                assert_eq!(events.stages.lock().unwrap().last(), Some(&Stage::Complete));
            }
        }
    }
}
#[test]
fn failed_and_cancelled_operations_preserve_existing_destinations() {
    let r = Renderer::new();
    let dir = tempfile::tempdir().unwrap();
    let (image, audio) = inputs(dir.path());
    let out = dir.path().join("keep.mp4");
    fs::write(&out, b"previous").unwrap();
    let token = CancellationToken::default();
    let req = RenderRequest {
        input: Input::Single {
            image: image.clone(),
            audio: audio.clone(),
        },
        settings: settings(),
        output: out.clone(),
        protected_paths: vec![out.clone()],
    };
    assert!(r.render(&req, &token, &()).is_err());
    assert_eq!(fs::read(&out).unwrap(), b"previous");
    let mut req = req;
    req.protected_paths.clear();
    token.cancel();
    assert!(matches!(r.render(&req, &token, &()), Err(Error::Cancelled)));
    assert_eq!(fs::read(&out).unwrap(), b"previous");
    let r = Renderer::new().with_options(OperationOptions {
        preview_timeout: Some(Duration::ZERO),
        ..Default::default()
    });
    let events = Events::default();
    assert!(matches!(
        r.preview(
            &PreviewRequest {
                image,
                output: out.clone(),
                settings: settings(),
                protected_paths: vec![]
            },
            &CancellationToken::default(),
            &events
        ),
        Err(Error::Deadline { .. })
    ));
    assert_eq!(fs::read(&out).unwrap(), b"previous");
    assert!(!events.stages.lock().unwrap().contains(&Stage::Complete));
    assert!(!fs::read_dir(dir.path()).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".avid-")));
}
#[test]
fn cancel_from_progress_waits_for_cleanup() {
    let r = Renderer::new();
    let dir = tempfile::tempdir().unwrap();
    let (image, audio) = inputs(dir.path());
    let output = dir.path().join("cancel.mp4");
    fs::write(&output, b"previous").unwrap();
    let token = CancellationToken::default();
    struct Cancel(CancellationToken);
    impl EventSink for Cancel {
        fn progress(&self, _: Progress) {
            self.0.cancel();
        }
    }
    let result = r.render(
        &RenderRequest {
            input: Input::Single { image, audio },
            settings: settings(),
            output: output.clone(),
            protected_paths: vec![],
        },
        &token,
        &Cancel(token.clone()),
    );
    assert!(matches!(result, Err(Error::Cancelled)));
    assert_eq!(fs::read(&output).unwrap(), b"previous");
}
#[test]
fn repeated_and_concurrent_previews_and_alias_rejection() {
    let r = Renderer::new();
    let dir = tempfile::tempdir().unwrap();
    let (image, _) = inputs(dir.path());
    std::thread::scope(|s| {
        let mut jobs = Vec::new();
        for i in 0..4 {
            let image = &image;
            let r = &r;
            let output = dir.path().join(format!("preview{i}.png"));
            jobs.push(s.spawn(move || {
                for _ in 0..3 {
                    r.preview(
                        &PreviewRequest {
                            image: image.clone(),
                            output: output.clone(),
                            settings: settings(),
                            protected_paths: vec![],
                        },
                        &CancellationToken::default(),
                        &(),
                    )
                    .unwrap();
                }
            }));
        }
        for job in jobs {
            job.join().unwrap();
        }
    });
    let original = fs::read(&image).unwrap();
    let alias = dir.path().join("alias.png");
    fs::hard_link(&image, &alias).unwrap();
    assert!(r
        .preview(
            &PreviewRequest {
                image: image.clone(),
                output: alias,
                settings: settings(),
                protected_paths: vec![]
            },
            &CancellationToken::default(),
            &()
        )
        .is_err());
    assert_eq!(fs::read(image).unwrap(), original);
}
#[test]
fn corrupt_and_nested_network_inputs_are_rejected() {
    let r = Renderer::new();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("nested.m3u8");
    fs::write(
        &file,
        b"#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXTINF:1,\nhttps://127.0.0.1:1/secret\n",
    )
    .unwrap();
    assert!(r
        .probe_audio_duration(&file, &CancellationToken::default())
        .is_err());
    fs::write(&file, b"corrupt artwork").unwrap();
    assert!(r
        .inspect_image(&file, &CancellationToken::default())
        .is_err());
}
