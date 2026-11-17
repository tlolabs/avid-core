#![cfg(feature = "legacy-subprocess")]
//! Deliberate reference comparison only. Production linked tests use no tools.
//! Supply both AVID_TEST_FFMPEG and AVID_TEST_FFPROBE as explicit absolute paths.
use avid_core::*;
use avid_media_bridge::{decode_audio, decode_video, probe, Control};
use std::{fs, io::Write, path::Path};

fn control() -> Control<'static> {
    Control {
        cancelled: &|| false,
        progress: &|_| {},
    }
}

fn artwork(path: &Path, reversed: bool) {
    let mut encoder = png::Encoder::new(fs::File::create(path).unwrap(), 96, 64);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let pixels: Vec<_> = (0..96 * 64)
        .flat_map(|index| {
            if (index % 96 < 48) ^ reversed {
                [220, 32, 16]
            } else {
                [16, 64, 220]
            }
        })
        .collect();
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&pixels)
        .unwrap();
}

fn audio(path: &Path) {
    let rate = 44100u32;
    let size = rate * 2;
    let mut file = fs::File::create(path).unwrap();
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
    for index in 0..rate {
        let sample = if index < rate / 8 {
            0
        } else {
            (12000.0 * (index as f64 * 440.0 * std::f64::consts::TAU / rate as f64).sin()) as i16
        };
        file.write_all(&sample.to_le_bytes()).unwrap();
    }
}

fn frames(path: &Path) -> Vec<(f64, Vec<u8>)> {
    let mut output = Vec::new();
    decode_video(path, &control(), &mut |frame| {
        assert_eq!((frame.width, frame.height), (160, 90));
        let mut pixels = Vec::with_capacity(160 * 90 * 3);
        for row in 0..90 {
            pixels.extend_from_slice(&frame.pixels[row * frame.stride..row * frame.stride + 480]);
        }
        output.push((frame.seconds.unwrap_or(0.0), pixels));
        true
    })
    .unwrap();
    output
}

fn mean_pixel_error(left: &[u8], right: &[u8]) -> f64 {
    assert_eq!(left.len(), right.len());
    left.iter()
        .zip(right)
        .map(|(a, b)| (i32::from(*a) - i32::from(*b)).unsigned_abs() as f64)
        .sum::<f64>()
        / left.len() as f64
}

fn pcm(path: &Path) -> Vec<f32> {
    let mut samples = Vec::new();
    decode_audio(path, 16000, 1, &control(), &mut |chunk| {
        samples.extend_from_slice(chunk.samples);
        true
    })
    .unwrap();
    samples
}

#[test]
#[ignore = "explicit historical FFmpeg/ffprobe pair required; never production fallback"]
fn linked_matches_reference_decoded_media_for_compositions_modes_and_hard_cuts() {
    let executable = std::env::var_os("AVID_TEST_FFMPEG").expect("Set AVID_TEST_FFMPEG");
    let probe_executable = std::env::var_os("AVID_TEST_FFPROBE").expect("Set AVID_TEST_FFPROBE");
    assert!(Path::new(&executable).is_absolute());
    assert!(Path::new(&probe_executable).is_absolute());
    let token = CancellationToken::default();
    let legacy =
        LegacyRenderer::new(MediaTools::from_paths(executable, probe_executable, &token).unwrap());
    let linked = Renderer::new();
    let root = tempfile::Builder::new()
        .prefix("avid reference ü ")
        .tempdir()
        .unwrap();
    let first = root.path().join("first.png");
    let second = root.path().join("second.png");
    let track = root.path().join("tone.wav");
    artwork(&first, false);
    artwork(&second, true);
    audio(&track);
    assert_eq!(
        linked.inspect_image(&first, &token).unwrap(),
        legacy.inspect_image(&first, &token).unwrap()
    );
    assert!(
        (linked
            .probe_audio_duration(&track, &token)
            .unwrap()
            .unwrap()
            - legacy
                .probe_audio_duration(&track, &token)
                .unwrap()
                .unwrap())
        .abs()
            < 0.001
    );
    for composition in [Composition::Fitted, Composition::SquarePadded] {
        let settings = RenderSettings {
            width: 160,
            height: 90,
            fps: 24,
            composition,
            flip_horizontal: true,
            flip_vertical: true,
            encoding: Encoding::Software,
            ..Default::default()
        };
        let mut preview = PreviewRequest {
            image: first.clone(),
            output: root.path().join("linked.png"),
            settings: settings.clone(),
            protected_paths: vec![],
        };
        linked.preview(&preview, &token, &()).unwrap();
        let expected_preview = root.path().join("reference.png");
        preview.output = expected_preview.clone();
        legacy.preview(&preview, &token, &()).unwrap();
        // Identical composition policy; allow rounding differences in scaling/SIMD.
        let pixel_error = mean_pixel_error(
            &frames(&root.path().join("linked.png"))[0].1,
            &frames(&expected_preview)[0].1,
        );
        assert!(
            pixel_error < 3.0,
            "preview {composition:?}: mean pixel error {pixel_error}"
        );
        for mode in [RenderMode::Simple, RenderMode::PerFrame] {
            for timeline in [false, true] {
                let input = if timeline {
                    Input::Timeline(
                        Timeline::new(vec![
                            Clip {
                                id: "first".into(),
                                image: first.clone(),
                                audio: track.clone(),
                                duration_seconds: 0.5,
                            },
                            Clip {
                                id: "second".into(),
                                image: second.clone(),
                                audio: track.clone(),
                                duration_seconds: 0.5,
                            },
                        ])
                        .unwrap(),
                    )
                } else {
                    Input::Single {
                        image: first.clone(),
                        audio: track.clone(),
                    }
                };
                let mut request = RenderRequest {
                    input,
                    settings: settings.clone(),
                    output: root.path().join("linked.mp4"),
                    protected_paths: vec![],
                };
                linked
                    .render_with_mode(&request, mode, &token, &())
                    .unwrap();
                let actual = request.output.clone();
                request.output = root.path().join("reference.mp4");
                legacy
                    .render_with_mode(&request, mode, &token, &())
                    .unwrap();
                let expected = request.output;
                let actual_info = probe(&actual, &control()).unwrap();
                let expected_info = probe(&expected, &control()).unwrap();
                assert_eq!(actual_info.sample_aspect_ratio, (1, 1));
                assert_eq!(
                    actual_info.sample_aspect_ratio,
                    expected_info.sample_aspect_ratio
                );
                assert_eq!(
                    (actual_info.audio_rate, actual_info.audio_channels),
                    (expected_info.audio_rate, expected_info.audio_channels)
                );
                assert!(
                    (actual_info.duration.unwrap() - expected_info.duration.unwrap()).abs() < 0.05
                );
                let actual_frames = frames(&actual);
                let expected_frames = frames(&expected);
                assert_eq!(actual_frames.len(), 24);
                assert_eq!(actual_frames.len(), expected_frames.len());
                for (actual, expected) in actual_frames.iter().zip(&expected_frames) {
                    assert!((actual.0 - expected.0).abs() < 0.001);
                    // Different valid encoder threading/GOP decisions alter compressed
                    // data. An RGB mean error below 4/255 preserves the composition
                    // while detecting flips, padding, geometry and wrong cut frames.
                    let error = mean_pixel_error(&actual.1, &expected.1);
                    assert!(
                        error < 4.0,
                        "{composition:?}/{mode:?}/timeline={timeline}: pixel error {error} at {}",
                        actual.0
                    );
                }
                let actual_audio = pcm(&actual);
                let expected_audio = pcm(&expected);
                // One AAC frame permits end padding; timestamps and initial silence
                // remain aligned. Do not compare compressed packets or padding bytes.
                assert!((actual_audio.len() as i64 - expected_audio.len() as i64).abs() <= 1024);
                let count = actual_audio.len().min(expected_audio.len());
                let rmse = (actual_audio[..count]
                    .iter()
                    .zip(&expected_audio[..count])
                    .map(|(a, b)| f64::from(a - b).powi(2))
                    .sum::<f64>()
                    / count as f64)
                    .sqrt();
                if rmse >= 0.02 {
                    if let Some(directory) = std::env::var_os("AVID_TEST_REFERENCE_EVIDENCE") {
                        fs::create_dir_all(&directory).unwrap();
                        fs::copy(&actual, Path::new(&directory).join("linked.mp4")).unwrap();
                        fs::copy(&expected, Path::new(&directory).join("reference.mp4")).unwrap();
                        fs::copy(&track, Path::new(&directory).join("source.wav")).unwrap();
                    }
                }
                assert!(
                    rmse < 0.02,
                    "{composition:?}/{mode:?}/timeline={timeline}: audio RMSE {rmse}"
                );
            }
        }
    }
}
