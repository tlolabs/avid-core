//! Decode linked outputs to compare media meaning (colors, timing and audio),
//! never compressed bytes. All fixtures are generated in-process.
use avid_core::*;
use avid_media_bridge::{decode_audio, decode_video, probe, Control};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
fn control() -> Control<'static> {
    Control {
        cancelled: &|| false,
        progress: &|_| {},
    }
}
fn png_file(path: &Path, color: [u8; 3]) {
    let mut e = png::Encoder::new(fs::File::create(path).unwrap(), 32, 16);
    e.set_color(png::ColorType::Rgb);
    e.set_depth(png::BitDepth::Eight);
    e.write_header()
        .unwrap()
        .write_image_data(&color.repeat(32 * 16))
        .unwrap();
}
fn wave(path: &Path, seconds: u32) {
    wave_at_rate(path, seconds, 44100);
}
fn wave_at_rate(path: &Path, seconds: u32, rate: u32) {
    let bytes = seconds * rate * 2;
    let mut f = fs::File::create(path).unwrap();
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + bytes).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    for x in [1u16, 1] {
        f.write_all(&x.to_le_bytes()).unwrap();
    }
    f.write_all(&rate.to_le_bytes()).unwrap();
    f.write_all(&(rate * 2).to_le_bytes()).unwrap();
    for x in [2u16, 16] {
        f.write_all(&x.to_le_bytes()).unwrap();
    }
    f.write_all(b"data").unwrap();
    f.write_all(&bytes.to_le_bytes()).unwrap();
    for n in 0..seconds * rate {
        let v = if n < rate / 4 {
            0
        } else {
            (12000.0 * (n as f64 * 440.0 * std::f64::consts::TAU / rate as f64).sin()) as i16
        };
        f.write_all(&v.to_le_bytes()).unwrap();
    }
}
fn settings() -> RenderSettings {
    RenderSettings {
        width: 160,
        height: 90,
        fps: 24,
        encoding: Encoding::Software,
        ..Default::default()
    }
}
fn export(image: &Path, audio: &Path, output: &Path, mode: RenderMode) {
    Renderer::new()
        .render_with_mode(
            &RenderRequest {
                input: Input::Single {
                    image: image.into(),
                    audio: audio.into(),
                },
                settings: settings(),
                output: output.into(),
                protected_paths: vec![],
            },
            mode,
            &CancellationToken::default(),
            &(),
        )
        .unwrap();
}
fn rgb_frames(path: &Path) -> Vec<(f64, [u8; 3])> {
    let mut frames = Vec::new();
    decode_video(path, &control(), &mut |f| {
        let x = f.width as usize / 2;
        let y = f.height as usize / 2;
        let n = y * f.stride + x * 3;
        frames.push((f.seconds.unwrap(), f.pixels[n..n + 3].try_into().unwrap()));
        true
    })
    .unwrap();
    frames
}
fn blue(rgb: [u8; 3]) -> bool {
    i32::from(rgb[2]) - i32::from(rgb[0]) > 50
}
#[test]
fn decoded_outputs_preserve_cadence_audio_layout_silence_and_hard_cuts() {
    let dir = tempfile::tempdir().unwrap();
    let red = dir.path().join("red.png");
    let blue_path = dir.path().join("blue.png");
    let audio = dir.path().join("audio.wav");
    png_file(&red, [255, 0, 0]);
    png_file(&blue_path, [0, 0, 255]);
    wave(&audio, 1);
    let output = dir.path().join("single.mp4");
    export(&red, &audio, &output, RenderMode::PerFrame);
    let metadata = probe(&output, &control()).unwrap();
    assert_eq!((metadata.audio_rate, metadata.audio_channels), (44100, 1));
    assert_eq!(metadata.sample_aspect_ratio, (1, 1));
    let frames = rgb_frames(&output);
    assert_eq!(frames.len(), 24);
    for (n, (time, rgb)) in frames.iter().enumerate() {
        assert!((time - n as f64 / 24.0).abs() < 0.001);
        assert!(rgb[0] > rgb[2] + 50);
    }
    let mut samples = Vec::new();
    decode_audio(&output, 16000, 1, &control(), &mut |chunk| {
        samples.extend_from_slice(chunk.samples);
        true
    })
    .unwrap();
    assert!((samples.len() as i64 - 16000).abs() < 500);
    assert!(samples[..3000].iter().all(|s| s.abs() < 0.01));
    assert!(samples[6000..9000].iter().any(|s| s.abs() > 0.1));
    let output = dir.path().join("timeline.mp4");
    Renderer::new()
        .render(
            &RenderRequest {
                input: Input::Timeline(
                    Timeline::new(vec![
                        Clip {
                            id: "red".into(),
                            image: red,
                            audio: audio.clone(),
                            duration_seconds: 0.5,
                        },
                        Clip {
                            id: "blue".into(),
                            image: blue_path,
                            audio,
                            duration_seconds: 0.5,
                        },
                    ])
                    .unwrap(),
                ),
                settings: settings(),
                output: output.clone(),
                protected_paths: vec![],
            },
            &CancellationToken::default(),
            &(),
        )
        .unwrap();
    let metadata = probe(&output, &control()).unwrap();
    assert_eq!((metadata.audio_rate, metadata.audio_channels), (48000, 2));
    let frames = rgb_frames(&output);
    assert_eq!(frames.len(), 24);
    assert!(!blue(frames[5].1));
    assert!(blue(frames[18].1));
}
fn animations(root: &Path) -> Vec<(PathBuf, u32)> {
    let gif = root.join("GIF renamed.png");
    let mut bytes = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\xff\x00\x00\x00\x00\xff".to_vec();
    bytes.extend(b"\x21\xff\x0bNETSCAPE2.0\x03\x01\x00\x00\x00");
    for pixel in [0x44, 0x4c] {
        bytes.extend(
            b"\x21\xf9\x04\x00\x14\x00\x00\x00\x2c\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02",
        );
        bytes.extend([pixel, 0x01, 0x00]);
    }
    bytes.push(0x3b);
    fs::write(&gif, bytes).unwrap();
    let mut paths = vec![(gif, 0)];
    for plays in [0, 1, 2] {
        let path = root.join(format!("APNG-{plays}.gif"));
        let mut e = png::Encoder::new(fs::File::create(&path).unwrap(), 1, 1);
        e.set_color(png::ColorType::Rgb);
        e.set_depth(png::BitDepth::Eight);
        e.set_animated(2, plays).unwrap();
        e.set_frame_delay(1, 5).unwrap();
        let mut w = e.write_header().unwrap();
        w.write_image_data(&[255, 0, 0]).unwrap();
        w.write_image_data(&[0, 0, 255]).unwrap();
        w.finish().unwrap();
        paths.push((path, plays));
    }
    paths
}
#[test]
fn decoded_gif_apng_loop_hold_and_first_frame_semantics() {
    let dir = tempfile::tempdir().unwrap();
    let audio = dir.path().join("audio.wav");
    wave(&audio, 1);
    let still = dir.path().join("green.png");
    png_file(&still, [0, 255, 0]);
    for (image, plays) in animations(dir.path()) {
        for mode in [RenderMode::Simple, RenderMode::PerFrame] {
            for timeline in [false, true] {
                let output = dir.path().join("animation.mp4");
                let input = if timeline {
                    Input::Timeline(
                        Timeline::new(vec![
                            Clip {
                                id: "static".into(),
                                image: still.clone(),
                                audio: audio.clone(),
                                duration_seconds: 0.5,
                            },
                            Clip {
                                id: "animated".into(),
                                image: image.clone(),
                                audio: audio.clone(),
                                duration_seconds: 1.0,
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
                Renderer::new()
                    .render_with_mode(
                        &RenderRequest {
                            input,
                            settings: settings(),
                            output: output.clone(),
                            protected_paths: vec![],
                        },
                        mode,
                        &CancellationToken::default(),
                        &(),
                    )
                    .unwrap();
                let frames = rgb_frames(&output);
                assert_eq!(
                    frames.len(),
                    if timeline { 36 } else { 24 },
                    "{image:?} {mode:?} timeline={timeline}"
                );
                for (time, expect_blue) in [
                    (0.1, false),
                    (0.3, true),
                    (0.5, false),
                    (0.7, true),
                    (0.9, false),
                ] {
                    let offset = if timeline { 0.5 } else { 0.0 };
                    let (_, rgb) = frames.iter().find(|(t, _)| *t >= time + offset).unwrap();
                    let expected = mode == RenderMode::PerFrame
                        && (expect_blue || (plays > 0 && time >= 0.4 * f64::from(plays)));
                    assert_eq!(
                        blue(*rgb),
                        expected,
                        "{image:?} plays{plays} {mode:?} timeline={timeline} at{time}={rgb:?}"
                    );
                }
            }
        }
    }
}
#[test]
fn exif_orientation_precedes_user_flips_and_preserves_coded_inspection() {
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("orientation6.jpg");
    let pixels: Vec<_> = (0..32 * 16)
        .flat_map(|i| {
            if i % 32 < 16 {
                [255, 0, 0]
            } else {
                [0, 0, 255]
            }
        })
        .collect();
    let mut compressor = turbojpeg::Compressor::new().unwrap();
    compressor.set_quality(100).unwrap();
    compressor.set_subsamp(turbojpeg::Subsamp::None).unwrap();
    let jpeg = compressor
        .compress_to_vec(turbojpeg::Image {
            pixels: &pixels,
            width: 32,
            pitch: 32 * 3,
            height: 16,
            format: turbojpeg::PixelFormat::RGB,
        })
        .unwrap();
    let mut out = jpeg[..2].to_vec();
    let exif = b"Exif\0\0II\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0";
    out.extend([0xff, 0xe1]);
    out.extend(((exif.len() + 2) as u16).to_be_bytes());
    out.extend(exif);
    out.extend_from_slice(&jpeg[2..]);
    fs::write(&image, out).unwrap();
    let r = Renderer::new();
    assert_eq!(
        r.inspect_image(&image, &CancellationToken::default())
            .unwrap(),
        (32, 16)
    );
    for flip in [false, true] {
        let output = dir.path().join("orientation.png");
        r.preview(
            &PreviewRequest {
                image: image.clone(),
                output: output.clone(),
                settings: RenderSettings {
                    flip_vertical: flip,
                    ..settings()
                },
                protected_paths: vec![],
            },
            &CancellationToken::default(),
            &(),
        )
        .unwrap();
        let mut points = Vec::new();
        decode_video(&output, &control(), &mut |f| {
            for y in [22usize, 67] {
                let offset = y * f.stride + 80 * 3;
                points.push(<[u8; 3]>::try_from(&f.pixels[offset..offset + 3]).unwrap());
            }
            false
        })
        .unwrap();
        assert_eq!(blue(points[0]), flip, "{points:?}");
        assert_eq!(blue(points[1]), !flip, "{points:?}");
    }
}
#[test]
fn streaming_decode_honors_cancel_and_releases_before_return() {
    let dir = tempfile::tempdir().unwrap();
    let audio = dir.path().join("long.wav");
    wave(&audio, 60);
    let mut samples = 0;
    let mut largest = 0;
    decode_audio(&audio, 16000, 1, &control(), &mut |chunk| {
        samples += chunk.samples.len();
        largest = largest.max(chunk.samples.len());
        true
    })
    .unwrap();
    assert_eq!(samples, 60 * 16000);
    assert!(largest <= 16384);
    let cancelled = std::cell::Cell::new(false);
    let ctl = Control {
        cancelled: &|| cancelled.get(),
        progress: &|_| {},
    };
    let err = decode_audio(&audio, 16000, 1, &ctl, &mut |_| {
        cancelled.set(true);
        true
    })
    .unwrap_err();
    assert!(err.code < 0);
    fs::remove_file(&audio).unwrap();
}

#[test]
fn delayed_tracks_keep_the_common_source_clock_in_single_and_timeline_exports() {
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/linked/delayed-audio.mp4");
    let directory = tempfile::tempdir().unwrap();
    let image = directory.path().join("art.png");
    png_file(&image, [255, 0, 0]);
    let duration = Renderer::new()
        .probe_audio_duration(&source, &CancellationToken::default())
        .unwrap()
        .unwrap();
    assert!((duration - 1.25).abs() < 0.001, "duration={duration}");
    for timeline in [false, true] {
        let input = if timeline {
            Input::Timeline(
                Timeline::new(vec![Clip {
                    id: "delayed".into(),
                    image: image.clone(),
                    audio: source.clone(),
                    duration_seconds: 1.25,
                }])
                .unwrap(),
            )
        } else {
            Input::Single {
                image: image.clone(),
                audio: source.clone(),
            }
        };
        let output = directory.path().join("delay.mp4");
        Renderer::new()
            .render(
                &RenderRequest {
                    input,
                    settings: settings(),
                    output: output.clone(),
                    protected_paths: vec![],
                },
                &CancellationToken::default(),
                &(),
            )
            .unwrap();
        let frames = rgb_frames(&output);
        assert_eq!(frames.len(), 30);
        let mut samples = Vec::new();
        decode_audio(&output, 16000, 1, &control(), &mut |chunk| {
            samples.extend_from_slice(chunk.samples);
            true
        })
        .unwrap();
        assert!(
            samples[..3200].iter().all(|s| s.abs() < 0.015),
            "initial silence removed: timeline={timeline}"
        );
        let onset = samples.iter().position(|s| s.abs() > 0.025).unwrap() as f64 / 16000.0;
        assert!(
            (onset - 0.25).abs() < 0.03,
            "onset={onset} timeline={timeline}"
        );
        assert!((samples.len() as f64 / 16000.0 - 1.25).abs() < 0.04);
    }
}

#[test]
fn hevc_software_and_automatic_decode_and_explicit_hardware_never_silently_falls_back() {
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("hevc.png");
    let audio = dir.path().join("hevc.wav");
    png_file(&image, [255, 0, 0]);
    wave(&audio, 1);
    let renderer = Renderer::new();
    for encoding in [Encoding::Software, Encoding::Automatic, Encoding::Hardware] {
        let output = dir.path().join("hevc.mp4");
        fs::write(&output, b"previous").unwrap();
        let result = renderer.render(
            &RenderRequest {
                input: Input::Single {
                    image: image.clone(),
                    audio: audio.clone(),
                },
                settings: RenderSettings {
                    codec: Codec::Hevc,
                    encoding,
                    ..settings()
                },
                output: output.clone(),
                protected_paths: vec![],
            },
            &CancellationToken::default(),
            &(),
        );
        match result {
            Ok(_) => {
                // AV_CODEC_ID_HEVC is the stable FFmpeg public codec identifier.
                assert_eq!(probe(&output, &control()).unwrap().video_codec_id, 172);
                assert!(fs::read(&output).unwrap().windows(4).any(|b| b == b"hvc1"));
                let frames = rgb_frames(&output);
                assert_eq!(frames.len(), 24);
                assert!(frames.iter().all(|(_, rgb)| !blue(*rgb)));
            }
            Err(error) if encoding == Encoding::Hardware => {
                assert!(!matches!(error, Error::Fallback { .. }));
                assert_eq!(fs::read(&output).unwrap(), b"previous");
            }
            Err(error) => panic!("{encoding:?} HEVC failed: {error}"),
        }
    }
}

#[cfg(unix)]
#[test]
fn native_absolute_paths_preserve_colon_filenames() {
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("art:renamed.png");
    let audio = dir.path().join("audio:track.wav");
    let output = dir.path().join("result:export.mp4");
    png_file(&image, [255, 0, 0]);
    wave(&audio, 1);
    export(&image, &audio, &output, RenderMode::Simple);
    assert_eq!(rgb_frames(&output).len(), 24);
    let mut samples = 0;
    decode_audio(&output, 16000, 1, &control(), &mut |chunk| {
        samples += chunk.samples.len();
        true
    })
    .unwrap();
    assert!(samples >= 16000);
}

#[test]
fn changing_decoded_audio_parameters_are_rejected_before_resampling() {
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/linked/changing-audio-rate.aac");
    let mut samples = 0;
    let error = decode_audio(&source, 16000, 1, &control(), &mut |chunk| {
        samples += chunk.samples.len();
        true
    })
    .unwrap_err();
    assert!(samples > 0, "the valid initial segment must decode first");
    assert_eq!(error.operation, "validate decoded audio parameters");
    assert!(error.detail.contains("mid-stream audio"));
    assert!(error.code < 0);
}

#[test]
fn short_audio_segments_do_not_stall_longer_timeline_video() {
    let directory = tempfile::tempdir().unwrap();
    let red = directory.path().join("long-red.png");
    let blue_path = directory.path().join("long-blue.png");
    let audio = directory.path().join("short.wav");
    png_file(&red, [255, 0, 0]);
    png_file(&blue_path, [0, 0, 255]);
    wave(&audio, 1);
    for mode in [RenderMode::Simple, RenderMode::PerFrame] {
        let output = directory.path().join("short-audio.mp4");
        Renderer::new()
            .with_options(OperationOptions {
                render_timeout: Some(std::time::Duration::from_secs(10)),
                ..Default::default()
            })
            .render_with_mode(
                &RenderRequest {
                    input: Input::Timeline(
                        Timeline::new(vec![
                            Clip {
                                id: "red".into(),
                                image: red.clone(),
                                audio: audio.clone(),
                                duration_seconds: 1.5,
                            },
                            Clip {
                                id: "blue".into(),
                                image: blue_path.clone(),
                                audio: audio.clone(),
                                duration_seconds: 1.5,
                            },
                        ])
                        .unwrap(),
                    ),
                    settings: settings(),
                    output: output.clone(),
                    protected_paths: vec![],
                },
                mode,
                &CancellationToken::default(),
                &(),
            )
            .unwrap();
        let frames = rgb_frames(&output);
        assert_eq!(frames.len(), 72, "mode={mode:?}");
        assert!(!blue(frames[30].1));
        assert!(blue(frames[42].1));
        let mut samples = Vec::new();
        decode_audio(&output, 16000, 1, &control(), &mut |chunk| {
            samples.extend_from_slice(chunk.samples);
            true
        })
        .unwrap();
        // Concat pads a non-final segment's short audio to its video endpoint.
        assert!(samples[18000..23000]
            .iter()
            .all(|sample| sample.abs() < 0.015));
        assert!(samples[30000..35000]
            .iter()
            .any(|sample| sample.abs() > 0.1));
    }
}

#[test]
fn fractional_sample_audio_end_never_submits_an_empty_or_nonfinal_partial_frame() {
    use avid_media_bridge::{execute, Input as MediaInput, MediaType, Plan, Repeat};
    let directory = tempfile::tempdir().unwrap();
    let image = directory.path().join("end.png");
    let audio = directory.path().join("end.wav");
    png_file(&image, [255, 0, 0]);
    wave(&audio, 1);
    for sample_end in [1024.2_f64, 1024.7, 2048.2, 2048.7] {
        let output = directory.path().join("sample-end.mp4");
        let plan = Plan {
            inputs: vec![
                MediaInput {
                    path: image.clone(),
                    media: MediaType::Video,
                    repeat: Repeat::Still,
                    fps: 24,
                    duration: None,
                },
                MediaInput {
                    path: audio.clone(),
                    media: MediaType::Audio,
                    repeat: Repeat::None,
                    fps: 24,
                    duration: None,
                },
            ],
            graph: "[0:v]scale=160:90,fps=24,format=yuv420p[video];[1:a:0]anull[audio]".into(),
            video_label: "video".into(),
            audio_label: Some("audio".into()),
            output: output.clone(),
            encoder: "libx264".into(),
            width: 160,
            height: 90,
            fps: 24,
            audio_bitrate: 128000,
            duration: Some(sample_end / 44100.0),
            preview: false,
            normalize_audio: false,
        };
        execute(&plan, &control()).unwrap();
        let duration = probe(&output, &control()).unwrap().duration.unwrap();
        assert!(
            (duration * 44100.0 - sample_end.round()).abs() <= 2.0,
            "requested samples={sample_end}, duration={duration}"
        );
        assert!(!rgb_frames(&output).is_empty());
    }
}

#[test]
fn high_sample_rate_inputs_negotiate_aac_without_changing_supported_source_rates() {
    let directory = tempfile::tempdir().unwrap();
    let image = directory.path().join("rates.png");
    png_file(&image, [255, 0, 0]);
    for (source_rate, expected_rate) in [
        (44100u32, 44100u32),
        (48000, 48000),
        (96000, 96000),
        (176400, 96000),
        (192000, 96000),
    ] {
        let audio = directory.path().join("rate.wav");
        let output = directory.path().join("rate.mp4");
        wave_at_rate(&audio, 1, source_rate);
        export(&image, &audio, &output, RenderMode::Simple);
        let metadata = probe(&output, &control()).unwrap();
        assert_eq!(
            metadata.audio_rate, expected_rate,
            "source rate={source_rate}"
        );
        assert_eq!(metadata.audio_channels, 1);
        let mut samples = Vec::new();
        decode_audio(&output, 16000, 1, &control(), &mut |chunk| {
            samples.extend_from_slice(chunk.samples);
            true
        })
        .unwrap();
        assert!((samples.len() as i64 - 16000).abs() < 500);
        assert!(samples[..3000].iter().all(|sample| sample.abs() < 0.015));
        assert!(samples[6000..9000].iter().any(|sample| sample.abs() > 0.1));
    }
}

#[test]
fn pathological_output_timestamps_fail_with_structured_errors_and_cleanup() {
    use avid_media_bridge::{execute, Input as MediaInput, MediaType, Plan, Repeat};
    let directory = tempfile::tempdir().unwrap();
    let image = directory.path().join("clock.png");
    let audio = directory.path().join("clock.wav");
    png_file(&image, [255, 0, 0]);
    wave(&audio, 1);
    let base = Plan {
        inputs: vec![
            MediaInput {
                path: image.clone(),
                media: MediaType::Video,
                repeat: Repeat::Still,
                fps: 24,
                duration: None,
            },
            MediaInput {
                path: audio.clone(),
                media: MediaType::Audio,
                repeat: Repeat::None,
                fps: 24,
                duration: None,
            },
        ],
        graph: "[0:v]scale=160:90,format=yuv420p[video];[1:a:0]anull[audio]".into(),
        video_label: "video".into(),
        audio_label: Some("audio".into()),
        output: directory.path().join("clock.mp4"),
        encoder: "libx264".into(),
        width: 160,
        height: 90,
        fps: 24,
        audio_bitrate: 128000,
        duration: Some(1.0),
        preview: false,
        normalize_audio: false,
    };
    let cases = [
        ("[0:v]scale=160:90,format=yuv420p,setpts=9.2233720368547748e18[video];[1:a:0]anull[audio]", Some(1.0), "normalize video timestamps"),
        ("[0:v]scale=160:90,format=yuv420p,setpts=-9.2233720368547748e18[video];[1:a:0]anull[audio]", Some(1.0), "normalize video timestamps"),
        ("[0:v]scale=160:90,format=yuv420p[video];[1:a:0]asetpts=-9.2233720368547748e18[audio]", Some(1.0), "normalize audio timestamps"),
        ("[0:v]scale=160:90,format=yuv420p[video];[1:a:0]anull[audio]", Some(1.0e20), "bound output audio duration"),
    ];
    for (graph, duration, operation) in cases {
        let mut plan = base.clone();
        plan.graph = graph.into();
        plan.duration = duration;
        // A wall deadline is a backstop against a regression that tries to
        // iterate an astronomical number of frames instead of rejecting it.
        let start = std::time::Instant::now();
        let control = Control {
            cancelled: &|| start.elapsed() > std::time::Duration::from_secs(3),
            progress: &|_| {},
        };
        let error = execute(&plan, &control).unwrap_err();
        assert_eq!(error.operation, operation, "graph={graph}: {error}");
        assert!(error.code < 0);
        assert!(!error.detail.is_empty());
        fs::remove_file(&plan.output).unwrap();
    }
    fs::remove_file(image).unwrap();
    fs::remove_file(audio).unwrap();
}

#[test]
fn webp_and_vorbis_sources_remain_supported() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ffmpeg");
    let image = fixtures.join("cover.webp");
    let audio = fixtures.join("vorbis.ogg");
    let renderer = Renderer::new();
    let token = CancellationToken::default();
    let dimensions = renderer.inspect_image(&image, &token).unwrap();
    assert!(dimensions.0 > 0 && dimensions.1 > 0);
    let duration = renderer
        .probe_audio_duration(&audio, &token)
        .unwrap()
        .unwrap();
    assert!(duration > 0.0);
    let root = tempfile::tempdir().unwrap();
    for mode in [RenderMode::Simple, RenderMode::PerFrame] {
        let output = root.path().join("webp-vorbis.mp4");
        export(&image, &audio, &output, mode);
        let metadata = probe(&output, &control()).unwrap();
        assert!(metadata.has_audio && metadata.has_video);
        assert!((metadata.duration.unwrap() - duration).abs() < 0.1);
        let mut count = 0;
        decode_video(&output, &control(), &mut |_| {
            count += 1;
            true
        })
        .unwrap();
        assert!(count > 0);
        let mut energy = 0.0f64;
        decode_audio(&output, 16000, 1, &control(), &mut |chunk| {
            energy += chunk
                .samples
                .iter()
                .map(|s| f64::from(*s) * f64::from(*s))
                .sum::<f64>();
            true
        })
        .unwrap();
        assert!(energy > 1.0);
    }
}

#[test]
fn existing_audio_fixture_formats_export_and_decode() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ffmpeg");
    let root = tempfile::tempdir().unwrap();
    let image = root.path().join("cover.png");
    png_file(&image, [255, 0, 0]);
    for name in ["audio.wma", "alac.m4a", "vorbis.ogg", "audio.opus"] {
        let audio = fixtures.join(name);
        let output = root.path().join(format!("{name}.mp4"));
        export(&image, &audio, &output, RenderMode::Simple);
        let metadata = probe(&output, &control()).unwrap();
        assert!(metadata.has_audio && metadata.has_video, "{name}");
        let mut samples = 0usize;
        decode_audio(&output, 16000, 1, &control(), &mut |chunk| {
            samples += chunk.samples.len();
            true
        })
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(samples > 1000, "{name}");
    }
}
