#![cfg(feature = "whisper")]
use avid_transcription::*;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};

/// Requires explicitly provisioned exact weights/audio; never downloads or invokes a helper.
#[test]
#[ignore = "requires AVID_TEST_WHISPER_MODEL and AVID_TEST_SPEECH"]
fn actual_offline_recognition_progress_offsets_repeated_jobs_and_cleanup() {
    let model = PathBuf::from(std::env::var_os("AVID_TEST_WHISPER_MODEL").expect("model fixture"));
    let audio = PathBuf::from(std::env::var_os("AVID_TEST_SPEECH").expect("speech fixture"));
    let directory = tempfile::tempdir().unwrap();
    let store = ModelStore::new(directory.path().join("cache")).unwrap();
    let manifest = ModelManifest::tiny_english();
    store.import(&manifest, &model, &|| false).unwrap();
    for _ in 0..2 {
        let lease = store.lease(&manifest, &|| false).unwrap();
        let saw_progress = AtomicBool::new(false);
        let transcript = recognize_audio(
            &lease,
            &audio,
            &RecognitionOptions {
                language: Some("en-US"),
                source_offset: 12.5,
                threads: 4,
            },
            &|| false,
            &|v| {
                if v.fraction.is_some() {
                    saw_progress.store(true, Ordering::Relaxed);
                }
            },
        )
        .unwrap();
        transcript.validate().unwrap();
        assert_eq!(
            transcript.provenance.model_identity,
            Some(manifest.identity().unwrap())
        );
        assert_eq!(
            transcript.provenance.runtime_revisions.as_ref().unwrap()["whisper.cpp"],
            WHISPER_REVISION
        );
        assert_eq!(
            transcript.provenance.runtime_build_sha256.as_deref(),
            Some(avid_whisper_bridge::BUILD_IDENTITY_SHA256)
        );
        assert!(saw_progress.load(Ordering::Relaxed));
        assert!(!transcript.segments.is_empty());
        let text = transcript
            .segments
            .iter()
            .map(|v| v.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        assert!(text.contains("hello"), "{text}");
        assert!(text.contains("local"), "{text}");
        assert!(transcript.segments[0].start_seconds.unwrap() >= 12.5);
        assert!(
            transcript.has_complete_word_timing(),
            "native word timing missing: {transcript:?}"
        );
        assert!(
            !caption_cues(&transcript, &Default::default(), &Default::default())
                .unwrap()
                .is_empty()
        );
        assert!(align_corrected_text(&lease, &audio, &transcript).is_err());
    }
    store.remove(&manifest).unwrap();
}
#[test]
#[ignore = "requires AVID_TEST_WHISPER_MODEL and AVID_TEST_SPEECH"]
fn actual_cancelled_loading_and_inference_release_model_lease() {
    let model = PathBuf::from(std::env::var_os("AVID_TEST_WHISPER_MODEL").unwrap());
    let audio = PathBuf::from(std::env::var_os("AVID_TEST_SPEECH").unwrap());
    let directory = tempfile::tempdir().unwrap();
    let store = ModelStore::new(directory.path().join("cache")).unwrap();
    let manifest = ModelManifest::tiny_english();
    store.import(&manifest, &model, &|| false).unwrap();
    {
        let lease = store.lease(&manifest, &|| false).unwrap();
        assert!(matches!(
            recognize_audio(&lease, &audio, &Default::default(), &|| true, &|_| {}),
            Err(Error::Cancelled)
        ));
    }
    {
        let lease = store.lease(&manifest, &|| false).unwrap();
        let cancelled = AtomicBool::new(false);
        let result = recognize_audio(
            &lease,
            &audio,
            &Default::default(),
            &|| cancelled.load(Ordering::Relaxed),
            &|v| {
                if v.stage == "recognizing" {
                    cancelled.store(true, Ordering::Relaxed)
                }
            },
        );
        assert!(matches!(result, Err(Error::Cancelled)));
    }
    store.remove(&manifest).unwrap();
}

#[test]
#[ignore = "requires AVID_TEST_WHISPER_MODEL"]
fn actual_corrupt_audio_keeps_native_media_error_context() {
    let model = PathBuf::from(std::env::var_os("AVID_TEST_WHISPER_MODEL").unwrap());
    let directory = tempfile::tempdir().unwrap();
    let store = ModelStore::new(directory.path().join("cache")).unwrap();
    let manifest = ModelManifest::tiny_english();
    store.import(&manifest, &model, &|| false).unwrap();
    let invalid = directory.path().join("corrupt.wav");
    std::fs::write(&invalid, b"this is not an audio file").unwrap();
    {
        let lease = store.lease(&manifest, &|| false).unwrap();
        let error =
            recognize_audio(&lease, &invalid, &Default::default(), &|| false, &|_| {}).unwrap_err();
        match error {
            Error::NativeMedia {
                code,
                operation,
                detail,
            } => {
                assert!(code < 0);
                assert!(!operation.is_empty());
                assert!(!detail.is_empty());
            }
            other => panic!("native media context was lost: {other}"),
        }
    }
    store.remove(&manifest).unwrap();
}
