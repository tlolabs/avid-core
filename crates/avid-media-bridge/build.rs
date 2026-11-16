mod link_order;
use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf};
fn main() {
    println!("cargo:rerun-if-env-changed=AVID_FFMPEG_PREFIX");
    println!("cargo:rerun-if-env-changed=AVID_FFMPEG_STATIC");
    println!("cargo:rerun-if-env-changed=AVID_FFMPEG_DEV");
    println!("cargo:rerun-if-changed=native/media.c");
    println!("cargo:rerun-if-changed=native/media.h");
    println!("cargo:rerun-if-changed=native/audio.c");
    println!("cargo:rerun-if-changed=native/audio.h");
    let prefix = PathBuf::from(env::var_os("AVID_FFMPEG_PREFIX").expect(
        "AVID_FFMPEG_PREFIX must identify an explicitly built linked FFmpeg SDK; no system fallback is used",
    ));
    assert!(prefix.is_absolute(), "AVID_FFMPEG_PREFIX must be absolute");
    let canonical_prefix = prefix.canonicalize().expect("FFmpeg prefix missing");
    let static_link = env::var("AVID_FFMPEG_STATIC").as_deref() == Ok("1");
    let development = env::var("AVID_FFMPEG_DEV").as_deref() == Ok("1");
    if !development {
        assert!(
            static_link,
            "Production linked SDKs use verified static linkage; set AVID_FFMPEG_STATIC=1"
        );
        let identity_path = prefix.join("linked-ffmpeg.json");
        println!("cargo:rerun-if-changed={}", identity_path.display());
        let identity: serde_json::Value = serde_json::from_slice(&fs::read(&identity_path)
            .expect("SDK identity missing; build with scripts/linked/build.py or explicitly mark a development SDK with AVID_FFMPEG_DEV=1"))
            .expect("Invalid linked SDK identity");
        assert_eq!(
            identity["schema"], 1,
            "Unsupported linked SDK identity schema"
        );
        assert_eq!(identity["kind"], "avid-linked-ffmpeg", "Not a linked SDK");
        assert_eq!(
            identity["target"].as_str(),
            env::var("TARGET").ok().as_deref(),
            "FFmpeg SDK target mismatch"
        );
        assert_eq!(
            identity["linkage"], "static",
            "Production SDK linkage mismatch"
        );
        assert_eq!(
            identity["ffmpeg_version"], "9.0.1",
            "Unpinned FFmpeg SDK version"
        );
        assert_eq!(
            identity["source_revision"], "bf1b838f2ab88b4f8fd83443325c782ea0e0f7fa",
            "Unpinned FFmpeg SDK revision"
        );
        let libraries = identity["libraries"]
            .as_object()
            .expect("SDK library hashes missing");
        for required in [
            "avformat",
            "avcodec",
            "avfilter",
            "avutil",
            "swscale",
            "swresample",
        ] {
            assert!(
                libraries.contains_key(&format!("lib/lib{required}.a")),
                "SDK lacks required library {required}"
            );
        }
        for (relative, expected) in libraries {
            let path = prefix.join(relative);
            let actual_path = path.canonicalize().expect("SDK library missing");
            assert!(
                actual_path.starts_with(&canonical_prefix),
                "SDK library escapes prefix"
            );
            println!("cargo:rerun-if-changed={}", path.display());
            let actual: String = Sha256::digest(fs::read(&path).expect("Cannot read SDK library"))
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            assert_eq!(
                Some(actual.as_str()),
                expected.as_str(),
                "SDK library hash mismatch: {relative}"
            );
        }
    } else {
        println!("cargo:warning=Using explicitly selected development FFmpeg SDK; this build is not a qualified production artifact");
    }
    // A build script is a separate process: these environment values affect its
    // pkg-config child only and prevent searching host SDKs during cross builds.
    env::set_var("PKG_CONFIG_LIBDIR", prefix.join("lib/pkgconfig"));
    env::set_var("PKG_CONFIG_PATH", "");
    env::set_var("PKG_CONFIG_ALLOW_CROSS", "1");
    let mut includes = vec![prefix.join("include")];
    let mut libraries = Vec::new();
    for lib in [
        "libavformat",
        "libavcodec",
        "libavfilter",
        "libavutil",
        "libswscale",
        "libswresample",
    ] {
        let found = pkg_config::Config::new()
            .statik(static_link)
            .cargo_metadata(false)
            .env_metadata(false)
            .probe(lib)
            .unwrap_or_else(|error| panic!("{lib} missing from explicit FFmpeg SDK: {error}"));
        for path in &found.include_paths {
            assert!(
                path.canonicalize()
                    .is_ok_and(|p| p.starts_with(&canonical_prefix)),
                "FFmpeg include escapes SDK: {}",
                path.display()
            );
            if !includes.contains(path) {
                includes.push(path.clone());
            }
        }
        libraries.push(found);
    }
    let mut build = cc::Build::new();
    build
        .file("native/media.c")
        .file("native/audio.c")
        .includes(&includes)
        .warnings(true)
        .flag_if_supported("-std=c11");
    build.compile("avid_linked_media");
    // C bridge must precede its providers in the native link order.
    let mut emitted = std::collections::HashSet::new();
    let library_groups: Vec<_> = libraries.iter().map(|found| found.libs.clone()).collect();
    for found in libraries {
        for path in found.link_paths {
            assert!(
                path.canonicalize()
                    .is_ok_and(|p| p.starts_with(&canonical_prefix)),
                "Native link path escapes SDK: {}",
                path.display()
            );
            if emitted.insert(format!("path:{}", path.display())) {
                println!("cargo:rustc-link-search=native={}", path.display());
            }
        }
        for framework in found.frameworks {
            if emitted.insert(format!("framework:{framework}")) {
                println!("cargo:rustc-link-lib=framework={framework}");
            }
        }
        for path in found.framework_paths {
            println!("cargo:rustc-link-search=framework={}", path.display());
        }
    }
    // Keep providers after all their dependents, including references introduced
    // by avfilter/swscale that were not present in avformat's first closure.
    for lib in link_order::libraries_in_link_order(&library_groups) {
        let archive = prefix.join("lib").join(format!("lib{lib}.a"));
        let kind = if static_link && archive.is_file() {
            "static="
        } else {
            ""
        };
        if !development && kind.is_empty() {
            if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") && lib == "c++_shared" {
                // libc++ is an NDK redistributable, never an Android system lib.
                // Its exact bundled bytes must be supplied by the pinned SDK.
                let identity: serde_json::Value = serde_json::from_slice(
                    &fs::read(prefix.join("linked-ffmpeg.json")).expect("SDK identity missing"),
                )
                .expect("Invalid SDK identity");
                let runtime = &identity["runtime_dependencies"]["libc++_shared.so"];
                let relative = runtime["path"]
                    .as_str()
                    .expect("Android SDK must bundle libc++_shared.so");
                let path = prefix.join(relative);
                assert!(
                    path.canonicalize()
                        .is_ok_and(|p| p.starts_with(&canonical_prefix)),
                    "Android runtime escapes SDK"
                );
                let actual: String =
                    Sha256::digest(fs::read(&path).expect("Android libc++ runtime missing"))
                        .iter()
                        .map(|v| format!("{v:02x}"))
                        .collect();
                assert_eq!(
                    runtime["sha256"].as_str(),
                    Some(actual.as_str()),
                    "Android libc++ runtime integrity mismatch"
                );
                println!("cargo:rerun-if-changed={}", path.display());
                println!(
                    "cargo:rustc-link-search=native={}",
                    path.parent().expect("runtime parent missing").display()
                );
            } else {
                assert!(
                    [
                        "c", "c++", "stdc++", "m", "dl", "pthread", "rt", "atomic", "gcc", "gcc_s",
                        "resolv", "iconv", "user32", "advapi32", "bcrypt", "ole32", "shell32",
                        "ws2_32", "secur32", "strmiids", "vfw32", "uuid", "mfplat", "mfuuid",
                        "oleaut32", "shlwapi", "gdi32"
                    ]
                    .contains(&lib.as_str()),
                    "Unpackaged non-system transitive library: {lib}"
                );
            }
        }
        println!("cargo:rustc-link-lib={kind}{lib}");
    }
}
