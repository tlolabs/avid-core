use avid_core::{CancellationToken, MediaTools};

#[test]
#[ignore = "requires AVID_RUNTIME_DIRECTORY pointing to a source-built managed runtime"]
fn source_built_runtime_satisfies_the_embedded_core_contract() {
    let path = std::env::var_os("AVID_RUNTIME_DIRECTORY").expect("set managed runtime directory");
    MediaTools::from_managed_directory(std::path::Path::new(&path), &CancellationToken::default())
        .expect("managed runtime must meet this Core revision's requirements");
}

#[cfg(unix)]
#[test]
fn managed_pair_rejects_missing_features_and_wrong_identity() {
    use avid_core::FFMPEG_RUNTIME_SPECIFICATION;
    use serde_json::{json, Value};
    use std::{fs, os::unix::fs::PermissionsExt};
    let d = tempfile::tempdir().unwrap();
    let spec: Value = serde_json::from_str(FFMPEG_RUNTIME_SPECIFICATION).unwrap();
    let os = if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x86_64"
    };
    let target = format!("{os}-{arch}");
    let build = json!({"target":target,"source_revision":spec["source"]["revision"],"recipe":spec["recipe"]});
    fs::write(d.path().join("spec.json"), FFMPEG_RUNTIME_SPECIFICATION).unwrap();
    fs::write(d.path().join("build.json"), build.to_string()).unwrap();
    for tool in ["ffmpeg", "ffprobe"] {
        let path = d.path().join(tool);
        fs::write(&path, format!("#!/bin/sh\nif [ \"$1\" = -version ]; then echo '{tool} version {}'; else echo ' V..... unrelated mentions libx264'; fi\n", spec["source"]["version"].as_str().unwrap())).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let error =
        MediaTools::from_managed_directory(d.path(), &CancellationToken::default()).unwrap_err();
    assert!(error.to_string().contains("Missing required"));
    fs::write(d.path().join("build.json"), "{}").unwrap();
    let error =
        MediaTools::from_managed_directory(d.path(), &CancellationToken::default()).unwrap_err();
    assert!(error.to_string().contains("build identity"));
    fs::write(d.path().join("spec.json"), "{}").unwrap();
    let error =
        MediaTools::from_managed_directory(d.path(), &CancellationToken::default()).unwrap_err();
    assert!(error.to_string().contains("specification differs"));
}

#[test]
#[ignore = "requires AVID_RUNTIME_DIRECTORY pointing to a source-built managed runtime"]
fn separate_resources_are_required_without_colocated_manifest_fallback() {
    let runtime = std::path::PathBuf::from(std::env::var_os("AVID_RUNTIME_DIRECTORY").unwrap());
    let resources = tempfile::tempdir().unwrap();
    for name in ["spec.json", "build.json"] {
        std::fs::copy(runtime.join(name), resources.path().join(name)).unwrap();
    }
    let token = CancellationToken::default();
    MediaTools::from_managed_layout(&runtime, resources.path(), &token).unwrap();
    std::fs::remove_file(resources.path().join("spec.json")).unwrap();
    assert!(MediaTools::from_managed_layout(&runtime, resources.path(), &token).is_err());
    std::fs::copy(
        runtime.join("spec.json"),
        resources.path().join("spec.json"),
    )
    .unwrap();
    assert!(MediaTools::from_managed_layout(resources.path(), resources.path(), &token).is_err());
}
