use avid_transcription::*;
use sha2::{Digest, Sha256};

fn fixture() -> Transcript {
    Transcript {
        segments: vec![Segment {
            text: "Hello world.".into(),
            words: vec![
                Word {
                    text: "Hello".into(),
                    start_seconds: Some(2.),
                    end_seconds: Some(2.4),
                    confidence: None,
                    timing_method: Some(TimingMethod::Recognizer),
                },
                Word {
                    text: "world.".into(),
                    start_seconds: Some(2.5),
                    end_seconds: Some(3.),
                    confidence: None,
                    timing_method: Some(TimingMethod::Recognizer),
                },
            ],
            start_seconds: Some(2.),
            end_seconds: Some(3.),
        }],
        language: Some("en".into()),
        provenance: Provenance::new("fixture", "fixture"),
        alignment_provenance: Some(Provenance::new("aligner", "fixture")),
    }
}
#[test]
fn corrections_are_atomic_and_never_retain_old_word_timing() {
    let original = fixture();
    let corrected = original.correcting(&[(0, "Corrected.".into())]).unwrap();
    assert!(corrected.segments[0].words.is_empty());
    assert!(corrected.alignment_provenance.is_none());
    assert!(original
        .correcting(&[(0, "one".into()), (1, "bad".into())])
        .is_err());
    assert!(original
        .correcting(&[(0, "one".into()), (0, "duplicate".into())])
        .is_err());
    assert_eq!(original.correcting(&[]).unwrap(), original);
    assert!(matches!(
        caption_cues(&corrected, &Default::default(), &Default::default()),
        Err(Error::MissingWordTiming)
    ));
}
#[test]
fn native_contract_roundtrips_and_timeline_captions_preserve_source() {
    let original = fixture();
    let data = serde_json::to_vec(&original).unwrap();
    assert_eq!(Transcript::from_json(&data).unwrap(), original);
    let cues = caption_cues(
        &original,
        &Default::default(),
        &CaptionWindow {
            source_start: 2.2,
            source_end: Some(2.9),
            timeline_start: 10.,
        },
    )
    .unwrap();
    assert_eq!(cues[0].start_seconds, 10.);
    assert!((cues[0].end_seconds - 10.7).abs() < 1e-9);
    assert!(export_captions(&cues, false)
        .unwrap()
        .contains("00:00:10,000 --> 00:00:10,700"));
    assert!(export_captions(&cues, true)
        .unwrap()
        .starts_with("WEBVTT\n\n"));
    assert_eq!(original.segments[0].words[0].start_seconds, Some(2.));
    assert!(original.offsetting(-3.).is_err());
    let mut bad = original;
    bad.segments[0].words[1].start_seconds = Some(1.);
    assert!(bad.validate().is_err());
}
#[test]
fn caption_markup_and_submillisecond_duration_are_safe() {
    let cues = vec![CaptionCue {
        start_seconds: 0.0001,
        end_seconds: 0.0002,
        text: "<b>&\ninjected".into(),
        words: vec![],
    }];
    let value = export_captions(&cues, true).unwrap();
    assert!(value.contains("00:00:00.000 --> 00:00:00.001"));
    assert!(value.contains("&lt;b&gt;&amp; injected"));
}

#[test]
fn caption_cues_reject_timeline_offsets_that_overflow_word_times() {
    let mut transcript = fixture();
    let segment = &mut transcript.segments[0];
    segment.words.truncate(1);
    segment.end_seconds = Some(f64::MAX);
    segment.words[0].end_seconds = segment.end_seconds;
    transcript.validate().unwrap();

    let window = CaptionWindow {
        timeline_start: f64::MAX / 2.,
        ..Default::default()
    };
    assert!(matches!(
        caption_cues(&transcript, &Default::default(), &window),
        Err(Error::InvalidInput(_))
    ));
}

fn manifest(bytes: &[u8]) -> ModelManifest {
    let mut m = ModelManifest::tiny_english();
    m.id = "fixture".into();
    m.file = "fixture.bin".into();
    m.size_bytes = bytes.len() as u64;
    m.sha256 = Sha256::digest(bytes)
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect();
    m
}
#[test]
fn model_import_verifies_copy_and_lease_excludes_removal_across_stores() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("模型 cache");
    let source = temporary.path().join("source.bin");
    std::fs::write(&source, b"model").unwrap();
    let manifest = manifest(b"model");
    let store = ModelStore::new(&root).unwrap();
    store.import(&manifest, &source, &|| false).unwrap();
    let lease = store.lease(&manifest, &|| false).unwrap();
    assert_eq!(std::fs::read(lease.path()).unwrap(), b"model");
    std::fs::write(&source, b"other").unwrap();
    assert_eq!(std::fs::read(lease.path()).unwrap(), b"model");
    let second = ModelStore::new(root).unwrap();
    let second_lease = second.lease(&manifest, &|| false).unwrap();
    assert!(matches!(second.remove(&manifest), Err(Error::Busy)));
    let installed = lease.path().to_owned();
    drop(lease);
    assert!(matches!(second.remove(&manifest), Err(Error::Busy)));
    drop(second_lease);
    std::fs::write(installed, b"other").unwrap();
    assert!(matches!(
        store.lease(&manifest, &|| false),
        Err(Error::Model(_))
    ));
    store.remove(&manifest).unwrap();
    assert!(store.lease(&manifest, &|| false).is_err());
}
#[test]
fn corrupt_cancelled_and_unsafe_model_imports_never_publish() {
    let temporary = tempfile::tempdir().unwrap();
    let store = ModelStore::new(temporary.path().join("cache")).unwrap();
    let source = temporary.path().join("source.bin");
    std::fs::write(&source, b"wrong").unwrap();
    let mut manifest = manifest(b"model");
    assert!(store.import(&manifest, &source, &|| false).is_err());
    assert!(store.lease(&manifest, &|| false).is_err());
    std::fs::write(&source, b"model").unwrap();
    assert!(matches!(
        store.import(&manifest, &source, &|| true),
        Err(Error::Cancelled)
    ));
    manifest.file = "../escape".into();
    assert!(store.import(&manifest, &source, &|| false).is_err());
}

#[test]
fn model_import_never_overwrites_the_payload_with_its_receipt() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("cache");
    let store = ModelStore::new(&root).unwrap();
    let source = temporary.path().join("source.bin");
    std::fs::write(&source, b"model").unwrap();
    let mut manifest = manifest(b"model");
    manifest.file = "model.json".into();

    assert!(store.import(&manifest, &source, &|| false).is_err());
    let installation = root
        .join(&manifest.id)
        .join(manifest.installation_id().unwrap());
    assert!(!installation.exists());
}

#[test]
fn reimport_checks_the_installed_receipt_as_well_as_the_model_bytes() {
    let temporary = tempfile::tempdir().unwrap();
    let store = ModelStore::new(temporary.path().join("cache")).unwrap();
    let source = temporary.path().join("source.bin");
    std::fs::write(&source, b"model").unwrap();
    let manifest = manifest(b"model");
    store.import(&manifest, &source, &|| false).unwrap();
    store.import(&manifest, &source, &|| false).unwrap();

    let lease = store.lease(&manifest, &|| false).unwrap();
    let receipt = lease.path().with_file_name("model.json");
    drop(lease);
    std::fs::remove_file(receipt).unwrap();

    assert!(store.import(&manifest, &source, &|| false).is_err());
    assert!(store.lease(&manifest, &|| false).is_err());
    store.remove(&manifest).unwrap();
    store.import(&manifest, &source, &|| false).unwrap();
    assert!(store.lease(&manifest, &|| false).is_ok());
}

fn pinned_fixture() -> Transcript {
    Transcript::from_json(include_bytes!(
        "../../../tests/fixtures/transcript-provenance-v1.json"
    ))
    .unwrap()
}

#[test]
fn shared_model_provenance_roundtrips_without_inventing_legacy_pins() {
    let original = pinned_fixture();
    let data = serde_json::to_vec(&original).unwrap();
    let decoded = Transcript::from_json(&data).unwrap();
    assert_eq!(decoded, original);
    let encoded: serde_json::Value = serde_json::from_slice(&data).unwrap();
    let shared: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../tests/fixtures/transcript-provenance-v1.json"
    ))
    .unwrap();
    for provenance in ["provenance", "alignmentProvenance"] {
        assert_eq!(encoded[provenance], shared[provenance]);
    }
    let identity = decoded.provenance.model_identity.as_ref().unwrap();
    assert_eq!(identity.schema_version, 1);
    assert_eq!(identity.artifacts.len(), 2);
    assert_ne!(
        identity.artifacts[0].revision,
        identity.artifacts[1].revision
    );
    assert_ne!(
        identity.artifacts[0].repository,
        identity.artifacts[1].repository
    );
    assert_eq!(
        decoded.provenance.runtime_revisions.as_ref().unwrap()["whisper.cpp"],
        WHISPER_REVISION
    );
    assert_eq!(
        decoded.provenance.runtime_build_sha256,
        Some("2".repeat(64))
    );
    assert_eq!(
        decoded
            .alignment_provenance
            .as_ref()
            .unwrap()
            .runtime_build_sha256,
        Some("3".repeat(64))
    );
    let legacy = fixture();
    let legacy_data = serde_json::to_value(&legacy).unwrap();
    assert!(legacy_data["provenance"].get("modelIdentity").is_none());
    assert!(legacy_data["provenance"].get("runtimeRevisions").is_none());
    assert!(legacy_data["provenance"]
        .get("runtimeBuildSHA256")
        .is_none());
    let legacy = Transcript::from_json(&serde_json::to_vec(&legacy_data).unwrap()).unwrap();
    assert!(legacy.provenance.model_identity.is_none());
    assert!(legacy.provenance.runtime_revisions.is_none());
    assert!(legacy.provenance.runtime_build_sha256.is_none());
    assert!(legacy
        .alignment_provenance
        .unwrap()
        .model_identity
        .is_none());
}

#[test]
fn model_identity_uses_the_unchanged_complete_manifest_digest() {
    let manifest = ModelManifest::tiny_english();
    let digest: String = Sha256::digest(serde_json::to_vec(&manifest).unwrap())
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect();
    let identity = manifest.identity().unwrap();
    assert_eq!(
        identity.installation_id,
        manifest.installation_id().unwrap()
    );
    assert_eq!(
        identity.installation_id,
        format!("{}-{digest}", manifest.revision)
    );
    assert_eq!(identity.manifest_sha256, digest);
    assert_eq!(identity.artifacts.len(), 1);
    assert_eq!(identity.artifacts[0].path, manifest.file);
    assert_eq!(identity.artifacts[0].repository_path, manifest.file);
    assert_eq!(identity.artifacts[0].sha256, manifest.sha256);
    assert_eq!(identity.artifacts[0].size_bytes, manifest.size_bytes);
    let mut changed = manifest.clone();
    changed.model_license.push_str(" fixture");
    assert_ne!(changed.identity().unwrap(), identity);
}

#[test]
fn offsets_corrections_and_captions_preserve_pinned_recognition_identity() {
    let original = pinned_fixture();
    let shifted = original.offsetting(8.).unwrap();
    assert_eq!(shifted.provenance, original.provenance);
    assert_eq!(shifted.alignment_provenance, original.alignment_provenance);
    let corrected = original
        .correcting(&[(0, "Corrected fixture.".into())])
        .unwrap();
    assert_eq!(corrected.provenance, original.provenance);
    assert!(corrected.alignment_provenance.is_none());
    assert_eq!(original.correcting(&[]).unwrap(), original);
    let before = original.clone();
    assert!(
        !caption_cues(&original, &Default::default(), &Default::default())
            .unwrap()
            .is_empty()
    );
    assert_eq!(original, before);
}

#[test]
fn malformed_optional_model_and_runtime_pins_are_rejected_in_both_provenances() {
    let original = serde_json::to_value(pinned_fixture()).unwrap();
    let malformed = [
        ("/engineID", serde_json::json!("  ")),
        ("/modelID", serde_json::json!("\n\t")),
        ("/modelID", serde_json::json!("bad\0identity")),
        ("/modelIdentity/schemaVersion", serde_json::json!(2)),
        ("/modelIdentity/revision", serde_json::json!("main")),
        ("/modelIdentity/manifestSHA256", serde_json::json!("bad")),
        ("/modelIdentity/installationID", serde_json::json!("wrong")),
        ("/modelIdentity/repository", serde_json::json!("../weights")),
        ("/modelIdentity/artifacts", serde_json::json!([])),
        (
            "/modelIdentity/artifacts/0/repository",
            serde_json::json!("org/repo/extra"),
        ),
        (
            "/modelIdentity/artifacts/0/revision",
            serde_json::json!("A".repeat(40)),
        ),
        (
            "/modelIdentity/artifacts/0/path",
            serde_json::json!("../escape"),
        ),
        (
            "/modelIdentity/artifacts/0/repositoryPath",
            serde_json::json!("C:\\escape"),
        ),
        (
            "/modelIdentity/artifacts/0/sizeBytes",
            serde_json::json!(u64::MAX),
        ),
        (
            "/modelIdentity/artifacts/0/sha256",
            serde_json::json!("f".repeat(63)),
        ),
        ("/runtimeRevisions", serde_json::json!({})),
        (
            "/runtimeRevisions",
            serde_json::json!({"whisper.cpp": "main"}),
        ),
        (
            "/runtimeRevisions",
            serde_json::json!({"../runtime": WHISPER_REVISION}),
        ),
        ("/runtimeBuildSHA256", serde_json::json!("not-a-sha256")),
    ];
    for provenance in ["provenance", "alignmentProvenance"] {
        for (path, value) in &malformed {
            let mut bad = original.clone();
            *bad.pointer_mut(&format!("/{provenance}{path}")).unwrap() = value.clone();
            assert!(
                Transcript::from_json(&serde_json::to_vec(&bad).unwrap()).is_err(),
                "{provenance}{path}: {value}"
            );
        }
    }
    let mut duplicate = pinned_fixture();
    let identity = duplicate.provenance.model_identity.as_mut().unwrap();
    identity.artifacts.push(identity.artifacts[0].clone());
    assert!(duplicate.validate().is_err());
    let mut ancestor = pinned_fixture();
    let identity = ancestor.provenance.model_identity.as_mut().unwrap();
    let mut artifact = identity.artifacts[0].clone();
    artifact.path = "TOKENIZER".into();
    identity.artifacts.push(artifact);
    assert!(ancestor.validate().is_err());
    let identity = duplicate.provenance.model_identity.as_mut().unwrap();
    identity
        .artifacts
        .last_mut()
        .unwrap()
        .path
        .make_ascii_uppercase();
    assert!(duplicate.validate().is_err());
    for path in [
        "",
        "/absolute",
        "weights//model",
        "weights/./model",
        "weights/../model",
        "weights/.avid-private",
        "weights/model\0.bin",
        "weights/model\n.bin",
        "weights\\model",
        "weights/model:1",
    ] {
        let mut bad = pinned_fixture();
        bad.provenance.model_identity.as_mut().unwrap().artifacts[0].path = path.into();
        assert!(bad.validate().is_err(), "{path:?}");
    }
    let mut valid = pinned_fixture();
    valid.provenance.model_identity.as_mut().unwrap().artifacts[0].size_bytes = 0;
    valid.provenance.model_identity.as_mut().unwrap().artifacts[0].path =
        "模型 files/.weights.bin".into();
    valid.validate().unwrap();
}
