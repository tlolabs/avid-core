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

/// Native coverage, including Windows: installed executables must release their
/// handles on cancellation/timeout so replacement, rollback and cleanup work.
#[test]
#[ignore = "requires AVID_RUNTIME_DIRECTORY pointing to a source-built managed runtime"]
fn installed_runtime_render_replacement_rollback_and_cleanup() {
    use avid_core::*;
    use std::{fs, path::Path, process::Command, time::Duration};
    fn captured(command: &mut Command) -> std::process::Output {
        use std::process::Stdio;
        let child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id();
        eprintln!("fixture_child_spawn pid={pid}");
        let output = child.wait_with_output().unwrap();
        eprintln!(
            "fixture_child_wait_and_drop pid={pid} status={:?}",
            output.status
        );
        output
    }
    fn stage(source: &Path, destination: &Path) {
        fs::create_dir(destination).unwrap();
        for name in [
            format!("ffmpeg{}", std::env::consts::EXE_SUFFIX),
            format!("ffprobe{}", std::env::consts::EXE_SUFFIX),
            "spec.json".into(),
            "build.json".into(),
        ] {
            fs::copy(source.join(&name), destination.join(&name)).unwrap();
            assert_eq!(
                fs::read(source.join(&name)).unwrap(),
                fs::read(destination.join(&name)).unwrap()
            );
        }
    }
    struct CancelAtPublication(CancellationToken);
    impl EventSink for CancelAtPublication {
        fn stage(&self, stage: Stage) {
            if stage == Stage::Publishing {
                self.0.cancel();
            }
        }
    }
    let source = std::path::PathBuf::from(std::env::var_os("AVID_RUNTIME_DIRECTORY").unwrap());
    let root = tempfile::tempdir().unwrap();
    let staged = root.path().join("staged ü runtime");
    let installed = root.path().join("installed ü runtime");
    let backup = root.path().join("previous runtime");
    stage(&source, &staged);
    fs::rename(&staged, &installed).unwrap();
    let tools =
        MediaTools::from_managed_directory(&installed, &CancellationToken::default()).unwrap();
    let image = root.path().join("artwork ü.png");
    let audio = root.path().join("track ü.wav");
    for (input, extra, output) in [
        ("testsrc2=s=192x128", vec!["-frames:v", "1"], &image),
        (
            "sine=frequency=523:duration=1",
            vec!["-c:a", "pcm_s16le"],
            &audio,
        ),
    ] {
        let result = captured(
            Command::new(tools.ffmpeg())
                .args(["-nostdin", "-v", "error", "-f", "lavfi", "-i", input])
                .args(extra)
                .arg(output),
        );
        assert!(result.status.success(), "{result:?}");
    }
    let renderer = Renderer::new(tools);
    let request = RenderRequest {
        input: Input::Single { image, audio },
        settings: RenderSettings {
            width: 160,
            height: 90,
            fps: 24,
            ..Default::default()
        },
        output: root.path().join("result.mp4"),
        protected_paths: vec![],
    };
    fs::write(&request.output, b"previous output").unwrap();
    renderer
        .render(&request, &CancellationToken::default(), &())
        .unwrap();
    let completed = fs::read(&request.output).unwrap();
    assert_ne!(completed, b"previous output");
    let result = captured(
        Command::new(renderer.tools().ffprobe())
            .args(["-v", "error", "-show_streams", "-of", "json"])
            .arg(&request.output),
    );
    assert!(result.status.success(), "{result:?}");
    let probe: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert!(probe["streams"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["codec_name"] == "h264"));
    let token = CancellationToken::default();
    assert!(matches!(
        renderer.render(&request, &token, &CancelAtPublication(token.clone())),
        Err(Error::Cancelled)
    ));
    assert_eq!(fs::read(&request.output).unwrap(), completed);
    let renderer = renderer.with_options(OperationOptions {
        render_timeout: Some(Duration::ZERO),
        ..Default::default()
    });
    assert!(matches!(
        renderer.render(&request, &CancellationToken::default(), &()),
        Err(Error::Timeout(_))
    ));
    assert_eq!(fs::read(&request.output).unwrap(), completed);
    assert!(!fs::read_dir(root.path()).unwrap().any(|entry| entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".avid-")));
    stage(&source, &staged);
    MediaTools::from_managed_directory(&staged, &CancellationToken::default()).unwrap();
    eprintln!(
        "runtime_rename_begin test_pid={} cwd_inside_runtime={}",
        std::process::id(),
        std::env::current_dir().unwrap().starts_with(&installed)
    );
    if let Err(error) = fs::rename(&installed, &backup) {
        eprintln!(
            "runtime_replacement_failed source=<RUNTIME> destination=<BACKUP> os_error={:?}",
            error.raw_os_error()
        );
        if let Some(handle) = std::env::var_os("AVID_HANDLE_DIAGNOSTIC") {
            // Failure-only search restricted to this synthetic directory and descendants.
            let result = Command::new(handle)
                .args(["-accepteula", "-nobanner", "-g"])
                .arg(&installed)
                .output()
                .unwrap();
            // Never emit Handle's raw output: it contains machine-specific paths.
            let output = String::from_utf8_lossy(&result.stdout);
            let prefix = installed.to_string_lossy().to_lowercase();
            let mut matches = 0;
            for line in output.lines() {
                let lower = line.to_lowercase();
                if let Some((_, target)) = lower.split_once(&prefix) {
                    if !target.is_empty() && !target.starts_with('\\') {
                        continue;
                    }
                    let fields: Vec<_> = line.split_whitespace().collect();
                    if fields.len() < 3 || fields[1] != "pid:" || fields[2].parse::<u32>().is_err()
                    {
                        continue;
                    }
                    let name: String = fields[0]
                        .chars()
                        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
                        .collect();
                    let resource = match target.trim_matches('\\').trim() {
                        "" => "<RUNTIME>",
                        "ffmpeg.exe" => "<RUNTIME>/ffmpeg.exe",
                        "ffprobe.exe" => "<RUNTIME>/ffprobe.exe",
                        "spec.json" => "<RUNTIME>/spec.json",
                        "build.json" => "<RUNTIME>/build.json",
                        _ => "<RUNTIME>/<file>",
                    };
                    eprintln!(
                        "scoped_handle owner={name} pid={} target={resource}",
                        fields[2]
                    );
                    matches += 1;
                }
            }
            eprintln!(
                "scoped_handle_count={matches} inspector_success={}",
                result.status.success()
            );
        }
        if let Some(diagnostic) = std::env::var_os("AVID_LOCK_DIAGNOSTIC") {
            let result = Command::new(diagnostic)
                .args([
                    installed.clone(),
                    installed.join("ffmpeg.exe"),
                    installed.join("ffprobe.exe"),
                ])
                .output();
            if let Ok(result) = result {
                eprintln!("{}", String::from_utf8_lossy(&result.stdout));
                eprintln!("{}", String::from_utf8_lossy(&result.stderr));
            }
        }
        panic!("runtime replacement failed: {error}");
    }
    fs::rename(&staged, &installed).unwrap();
    MediaTools::from_managed_directory(&installed, &CancellationToken::default()).unwrap();
    // A damaged update must fail managed discovery instead of selecting another pair.
    fs::write(installed.join("build.json"), "{}").unwrap();
    assert!(MediaTools::from_managed_directory(&installed, &CancellationToken::default()).is_err());
    fs::remove_dir_all(&installed).unwrap();
    fs::rename(&backup, &installed).unwrap();
    MediaTools::from_managed_directory(&installed, &CancellationToken::default()).unwrap();
    // Explicit close surfaces Windows handle leaks instead of ignoring cleanup errors.
    root.close().unwrap();
}
