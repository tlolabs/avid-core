mod link_runtime;
use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=AVID_WHISPER_PREFIX");
    println!("cargo:rerun-if-changed=native/whisper_bridge.cpp");
    println!("cargo:rerun-if-changed=native/whisper_bridge.h");
    let prefix = PathBuf::from(env::var_os("AVID_WHISPER_PREFIX")
        .expect("AVID_WHISPER_PREFIX must identify the pinned Core-built whisper.cpp 1.8.2 prefix; no system fallback"));
    assert!(prefix.is_absolute(), "AVID_WHISPER_PREFIX must be absolute");
    let canonical = prefix.canonicalize().expect("whisper.cpp prefix missing");
    let identity_path = prefix.join("avid-whisper-build.json");
    println!("cargo:rerun-if-changed={}", identity_path.display());
    let identity_bytes = fs::read(&identity_path)
        .expect("Core whisper.cpp identity missing; use script/build_whisper.py");
    let identity_digest: String = Sha256::digest(&identity_bytes)
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect();
    println!("cargo:rustc-env=AVID_WHISPER_BUILD_IDENTITY_SHA256={identity_digest}");
    let identity: serde_json::Value =
        serde_json::from_slice(&identity_bytes).expect("invalid Core whisper.cpp identity");
    assert_eq!(
        identity["schema"], 1,
        "unsupported inference artifact schema"
    );
    assert_eq!(identity["kind"], "avid-linked-inference");
    assert_eq!(identity["runtime"], "whisper.cpp-1.8.2");
    assert_eq!(
        identity["source_revision"],
        "4979e04f5dcaccb36057e059bbaed8a2f5288315"
    );
    assert_eq!(
        identity["source_archive_sha256"],
        "ec8e33f4c838ed459ad0a23da255c0262f150a6f54839a17e504ddf1495305d5"
    );
    assert_eq!(
        identity["target"].as_str(),
        env::var("TARGET").ok().as_deref(),
        "inference SDK target mismatch"
    );
    assert_eq!(identity["profile"], "cpu-offline");
    // The original archive alone does not identify the exception-safe runtime.
    // Require the current patch and its retained corresponding source/notices.
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let manifest_path = root.join("licenses/linked-inference/patches/manifest.json");
    println!("cargo:rerun-if-changed={}", manifest_path.display());
    let patch_spec: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("Whisper patch manifest missing"))
            .expect("invalid Whisper patch manifest");
    assert_eq!(patch_spec["variant"], "avid-exception-safety-r1");
    assert_eq!(identity["sdk_variant"], patch_spec["variant"]);
    assert_eq!(identity["source_patches"], serde_json::json!([patch_spec]));
    assert_eq!(identity["sdk_license"], "MIT AND GPL-3.0-or-later");
    let patch_name = patch_spec["patch"]
        .as_str()
        .expect("Whisper patch name missing");
    assert_eq!(patch_name, "avid-exception-safety-r1.patch.txt");
    for (installed, original) in [
        (
            patch_name,
            root.join("licenses/linked-inference/patches")
                .join(patch_name),
        ),
        ("AVID-EXCEPTION-PATCH-GPL-3.0.txt", root.join("LICENSE")),
        (
            "whisper-MIT.txt",
            root.join("licenses/linked-inference/whisper.cpp-MIT.txt"),
        ),
    ] {
        let retained = prefix.join("licenses").join(installed);
        assert!(
            retained
                .canonicalize()
                .is_ok_and(|p| p.starts_with(&canonical)),
            "inference patch/notice escapes SDK"
        );
        println!("cargo:rerun-if-changed={}", retained.display());
        println!("cargo:rerun-if-changed={}", original.display());
        let expected = fs::read(&original).expect("Whisper patch/notice source missing");
        if installed == patch_name {
            let hash: String = Sha256::digest(&expected)
                .iter()
                .map(|v| format!("{v:02x}"))
                .collect();
            assert_eq!(
                patch_spec["patch_sha256"].as_str(),
                Some(hash.as_str()),
                "Whisper patch source integrity mismatch"
            );
        }
        assert_eq!(
            fs::read(&retained).expect("retained Whisper patch/notice missing"),
            expected,
            "inference patch/notice differs from current source"
        );
    }
    let retained_manifest = prefix.join("licenses/patch-manifest.json");
    println!("cargo:rerun-if-changed={}", retained_manifest.display());
    assert!(
        retained_manifest
            .canonicalize()
            .is_ok_and(|p| p.starts_with(&canonical)),
        "inference patch manifest escapes SDK"
    );
    let retained_spec: serde_json::Value = serde_json::from_slice(
        &fs::read(retained_manifest).expect("retained Whisper patch manifest missing"),
    )
    .expect("invalid retained Whisper patch manifest");
    assert_eq!(
        retained_spec, patch_spec,
        "inference patch manifest differs from current source"
    );
    let libraries = identity["libraries"]
        .as_object()
        .expect("inference artifact hashes missing");
    for library in ["whisper", "ggml", "ggml-cpu", "ggml-base"] {
        let name = format!("lib{library}.a");
        let name = if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
            format!("{library}.lib")
        } else {
            name
        };
        let path = prefix.join("lib").join(&name);
        assert!(
            path.canonicalize().is_ok_and(|v| v.starts_with(&canonical)),
            "inference library escapes prefix"
        );
        println!("cargo:rerun-if-changed={}", path.display());
        let digest: String =
            Sha256::digest(fs::read(&path).expect("missing pinned inference library"))
                .iter()
                .map(|v| format!("{v:02x}"))
                .collect();
        assert_eq!(
            libraries.get(&name).and_then(|v| v.as_str()),
            Some(digest.as_str()),
            "inference library integrity mismatch"
        );
    }
    let version = fs::read_to_string(prefix.join("lib/pkgconfig/whisper.pc"))
        .expect("whisper.cpp install metadata is missing");
    assert!(
        version.lines().any(|v| v.trim() == "Version: 1.8.2"),
        "whisper.cpp must be pinned 1.8.2"
    );
    cc::Build::new()
        .file("native/whisper_bridge.cpp")
        .include(prefix.join("include"))
        .cpp(true)
        .cpp_link_stdlib(None)
        .flag_if_supported("-std=c++17")
        .warnings(true)
        .compile("avid_whisper");
    println!(
        "cargo:rustc-link-search=native={}",
        prefix.join("lib").display()
    );
    for lib in ["whisper", "ggml", "ggml-cpu", "ggml-base"] {
        println!("cargo:rustc-link-lib=static={lib}");
    }
    let target = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap();
    let runtime = link_runtime::cpp_runtime(&target, &target_env, identity["cxx_runtime"].as_str())
        .expect("Inference C++ runtime identity invalid");
    if target == "android" {
        let redistributable = &identity["runtime_dependencies"]["libc++_shared.so"];
        let path = prefix.join(
            redistributable["path"]
                .as_str()
                .expect("Missing bundled NDK libc++ runtime"),
        );
        assert!(
            path.canonicalize().is_ok_and(|p| p.starts_with(&canonical)),
            "NDK runtime escapes inference SDK"
        );
        let actual: String = Sha256::digest(fs::read(&path).expect("NDK runtime missing"))
            .iter()
            .map(|v| format!("{v:02x}"))
            .collect();
        assert_eq!(
            redistributable["sha256"].as_str(),
            Some(actual.as_str()),
            "NDK runtime hash mismatch"
        );
        // FFmpeg and Whisper must use one exact shared C++ implementation.
        let ffmpeg_prefix = PathBuf::from(
            env::var_os("AVID_FFMPEG_PREFIX")
                .expect("Android inference also requires its explicit linked media SDK"),
        );
        let ffmpeg: serde_json::Value = serde_json::from_slice(
            &fs::read(ffmpeg_prefix.join("linked-ffmpeg.json"))
                .expect("FFmpeg SDK identity missing"),
        )
        .expect("Invalid FFmpeg SDK identity");
        assert_eq!(
            ffmpeg["runtime_dependencies"]["libc++_shared.so"]["sha256"].as_str(),
            Some(actual.as_str()),
            "FFmpeg and Whisper use different Android C++ runtimes"
        );
        println!("cargo:rerun-if-changed={}", path.display());
        println!(
            "cargo:rustc-link-search=native={}",
            path.parent().unwrap().display()
        );
    }
    if let Some(runtime) = runtime {
        println!("cargo:rustc-link-lib={runtime}");
    }
    if matches!(target.as_str(), "macos" | "ios") {
        println!("cargo:rustc-link-lib=framework=Accelerate");
    } else if target != "windows" {
        println!("cargo:rustc-link-lib=m");
        if target != "android" {
            println!("cargo:rustc-link-lib=pthread");
        }
    }
}
