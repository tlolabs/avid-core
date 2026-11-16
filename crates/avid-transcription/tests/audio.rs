#![cfg(feature = "whisper")]
use avid_media_bridge::*;
use std::sync::atomic::{AtomicBool, Ordering};
fn wav(path: &std::path::Path, seconds: u32) {
    let frames = 48_000 * seconds;
    let size = frames * 4;
    let mut out = Vec::new();
    out.extend(b"RIFF");
    out.extend((size + 36).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(2u16.to_le_bytes());
    out.extend(48_000u32.to_le_bytes());
    out.extend(192_000u32.to_le_bytes());
    out.extend(4u16.to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend(b"data");
    out.extend(size.to_le_bytes());
    for i in 0..frames {
        let value = (((i as f64 / 48_000. * 440. * std::f64::consts::TAU).sin()) * 12_000.) as i16;
        out.extend(value.to_le_bytes());
        out.extend(value.to_le_bytes());
    }
    std::fs::write(path, out).unwrap();
}
#[test]
fn linked_pcm_resampling_unicode_clock_and_flush_are_correct() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("音频 🎧.wav");
    wav(&path, 2);
    let control = Control {
        cancelled: &|| false,
        progress: &|_| {},
    };
    let mut count = 0;
    let mut energy = 0.;
    let mut maximum_chunk = 0;
    decode_audio(&path, 16_000, 1, &control, &mut |chunk| {
        assert!((chunk.start_seconds - count as f64 / 16_000.).abs() < 1e-9);
        assert_eq!(chunk.channels, 1);
        assert!(chunk.samples.iter().all(|v| v.is_finite()));
        maximum_chunk = maximum_chunk.max(chunk.samples.len());
        count += chunk.samples.len();
        energy += chunk
            .samples
            .iter()
            .map(|v| f64::from(*v) * f64::from(*v))
            .sum::<f64>();
        true
    })
    .unwrap();
    assert_eq!(count, 32_000);
    assert!(energy / count as f64 > 0.01);
    assert!(maximum_chunk <= 1_048_576);
}
#[test]
fn linked_audio_cancel_error_panic_and_remote_inputs_release_resources() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.wav");
    wav(&path, 2);
    let cancelled = AtomicBool::new(false);
    let control = Control {
        cancelled: &|| cancelled.load(Ordering::Relaxed),
        progress: &|_| {},
    };
    assert!(decode_audio(&path, 16_000, 1, &control, &mut |_| {
        cancelled.store(true, Ordering::Relaxed);
        true
    })
    .is_err());
    cancelled.store(false, Ordering::Relaxed);
    assert!(decode_audio(&path, 16_000, 1, &control, &mut |_| panic!(
        "callback failure"
    ))
    .is_err());
    let mut count = 0;
    decode_audio(&path, 16_000, 1, &control, &mut |v| {
        count += v.samples.len();
        true
    })
    .unwrap();
    assert_eq!(count, 32_000);
    assert!(decode_audio(
        std::path::Path::new("https://example.com/audio.wav"),
        16_000,
        1,
        &control,
        &mut |_| true
    )
    .is_err());
    assert!(decode_audio(&path, 0, 1, &control, &mut |_| true).is_err());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn supported_audio_formats_decode_on_their_source_clock() {
    let fixtures =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/ffmpeg");
    let control = Control {
        cancelled: &|| false,
        progress: &|_| {},
    };
    // Independent FFmpeg decoding with aresample:first_pts=0 yields these counts.
    // WMA retains its real 92 ms source delay and its final timestamp-less frame;
    // Vorbis has a 128/44100-second delay, while Opus priming is discarded.
    for (name, expected_count, initial_silence) in [
        ("audio.wma", 17_076usize, 1_472usize),
        ("alac.m4a", 16_000, 0),
        ("vorbis.ogg", 16_000, 46),
        ("audio.opus", 16_000, 0),
    ] {
        let mut count = 0;
        let mut energy = 0.;
        decode_audio(&fixtures.join(name), 16_000, 1, &control, &mut |chunk| {
            assert!(
                (chunk.start_seconds - count as f64 / 16_000.).abs() < 1e-9,
                "{name}"
            );
            assert_eq!(chunk.channels, 1, "{name}");
            assert!(chunk.samples.iter().all(|v| v.is_finite()), "{name}");
            let silence = initial_silence
                .saturating_sub(count)
                .min(chunk.samples.len());
            assert!(
                chunk.samples[..silence].iter().all(|v| *v == 0.),
                "{name}: source delay lost"
            );
            count += chunk.samples.len();
            energy += chunk
                .samples
                .iter()
                .map(|v| f64::from(*v) * f64::from(*v))
                .sum::<f64>();
            true
        })
        .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert!(
            count.abs_diff(expected_count) <= 1,
            "{name}: {count} samples, expected {expected_count}"
        );
        assert!(energy > 1., "{name}: no decoded tone");
    }
}

fn clock_fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/audio")
        .join(name)
}

#[test]
fn partial_codec_priming_is_discarded_without_shifting_valid_samples() {
    // The first AAC packet has PTS=-896 and skip_samples=896 of 1024 samples.
    // libavcodec must emit the remaining 128 samples at source time zero. This
    // differs from skipping a complete encoder-delay frame and exposes an unset
    // AVCodecContext.pkt_timebase even when ordinary AAC inputs appear to work.
    let control = Control {
        cancelled: &|| false,
        progress: &|_| {},
    };
    let mut samples = Vec::new();
    decode_audio(
        &clock_fixture("aac-partial-priming.m4a"),
        16_000,
        1,
        &control,
        &mut |chunk| {
            assert!((chunk.start_seconds - samples.len() as f64 / 16_000.).abs() < 1e-9);
            samples.extend_from_slice(chunk.samples);
            true
        },
    )
    .unwrap();
    assert!(
        samples.len().abs_diff(16_000) <= 1,
        "{} samples",
        samples.len()
    );
    assert!(samples.iter().all(|v| v.is_finite()));
    assert!(
        samples[..320].iter().any(|v| v.abs() > 0.02),
        "codec priming was incorrectly preserved as source silence"
    );
    assert!(
        samples
            .iter()
            .map(|v| f64::from(*v) * f64::from(*v))
            .sum::<f64>()
            > 1.
    );
}

#[test]
fn genuine_negative_or_discontinuous_clocks_still_fail_and_real_delay_is_preserved() {
    let control = Control {
        cancelled: &|| false,
        progress: &|_| {},
    };
    let mut emitted = 0;
    let error = decode_audio(
        &clock_fixture("negative-clock.mka"),
        48_000,
        1,
        &control,
        &mut |chunk| {
            emitted += chunk.samples.len();
            true
        },
    )
    .unwrap_err();
    assert!(
        error.detail.contains("negative initial audio timestamp"),
        "{error}"
    );
    assert_eq!(
        emitted, 0,
        "genuine negative PCM has no codec skip metadata"
    );
    let error = decode_audio(
        &clock_fixture("discontinuous-clock.mka"),
        48_000,
        1,
        &control,
        &mut |chunk| {
            emitted += chunk.samples.len();
            true
        },
    )
    .unwrap_err();
    assert!(error.detail.contains("discontinuous"), "{error}");
    assert!(emitted > 0 && emitted < 7_200);

    let mut samples = Vec::new();
    decode_audio(
        &clock_fixture("delayed-clock.mka"),
        48_000,
        1,
        &control,
        &mut |chunk| {
            assert!((chunk.start_seconds - samples.len() as f64 / 48_000.).abs() < 1e-9);
            samples.extend_from_slice(chunk.samples);
            true
        },
    )
    .unwrap();
    assert_eq!(
        samples.len(),
        13_200,
        "125 ms real delay plus 150 ms decoded PCM"
    );
    assert!(samples[..6_000].iter().all(|v| *v == 0.));
    assert!(samples[6_000..6_100].iter().any(|v| v.abs() > 0.02));
}
