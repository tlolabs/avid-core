# ATIV migration to avid-core 0.3.0

Based on read-only inspection of ATIV `f38c433076c2980ff356da3d54e6f9b465439e6d`
(branch `codex/core-runtime-migration`). This Core task did not modify ATIV.
Reinspect ATIV before implementing these instructions. The renderer is already
shared; this migration replaces its managed-runtime adapter and build coupling.
See [the public contract](integration.md).

1. In ATIV's root `Cargo.toml`, replace the sibling path workspace dependency with:

   ```toml
   avid-core = { git = "https://github.com/tlolabs/avid-core.git", tag = "v0.3.0", version = "=0.3.0" }
   ```

   Prefer `rev = "<full v0.3.0 commit>"` instead of `tag` for an explicit immutable
   source pin. Keep `avid-core.workspace = true` in `crates/ativ-core/Cargo.toml`.
   Remove `crates/ativ-core/build.rs`, which currently requires a sibling checkout
   whose HEAD matches `runtime/core-revision`. Regenerate and commit Cargo.lock
   (`cargo update -p avid-core`). Retire `runtime/core-revision` or retain it only
   as documentation of the Cargo source pin, never as an FFmpeg mapping.

2. In `crates/ativ-engine/src/media_tools.rs::resolve`, retain the host's executable
   location and CLI override policy. Remove metadata directory calculation and
   `MediaTools::from_managed_layout`. Resolve both actual paths in ATIV, then call:

   ```rust
   let suffix = std::env::consts::EXE_SUFFIX;
   let ffmpeg_path = ffmpeg.unwrap_or_else(|| directory.join(format!("ffmpeg{suffix}")));
   let ffprobe_path = ffprobe.unwrap_or_else(|| directory.join(format!("ffprobe{suffix}")));
   let tools = MediaTools::from_paths(ffmpeg_path, ffprobe_path, token)?;
   ```

   Here `directory` is resolved by ATIV for its chosen bundle layout. Resolve it
   before both the packaged and override branches; an explicit missing override
   must fail. If retaining development PATH discovery, keep it an explicit host
   mode. Production must not fall through to another pair. No Core manifests are
   required. Preserve `ativ-core`'s public re-exports and error mapping.

3. Keep `crates/ativ-core/src/model.rs::RenderRequest::shared` unchanged:
   `Input::Single`, `Codec::H264`, `Encoding::Software`, `Composition::Fitted`,
   original audio semantics, 1–240 fps and current bitrate syntax. Preserve the
   Simple/PerFrame selection, native UI and existing engine JSON protocol,
   presets, progress, cancellation and output protection.

4. Replace `script/core_runtime.py` and `script/acquire_core_runtime.py` with
   ATIV-owned acquisition/build, integrity verification, staging and package
   validation. They currently import Core `host.py`, read its recipe, and call
   Core `acquire.py`; those entry points were removed. Choose and pin ATIV's
   FFmpeg source/provider/checksums independently of the Rust library revision.
   Use one chosen pair throughout ATIV. Record binary provenance and signing
   hashes in an ATIV-owned format if needed; do not copy Core recipe manifests.

5. Update callers: `script/build_and_run.sh`, `package_macos.sh`,
   `package_windows.ps1`, `package_linux.sh`, `validate_package.py`,
   `test_engine_integration.sh`, `test_core_runtime.py`,
   `test_runtime_ownership.py`, and `package_core_candidate_macos.sh`.
   Replace assertions about Core manifests/publication with assertions about
   ATIV's selected pair, missing/damaged tools, version mismatch, capabilities
   and packaged execution. Retire the isolated Core-candidate packaging path or
   turn it into an ordinary ATIV package test.

6. In `.github/workflows/native-release.yml`, remove mandatory sibling Core
   checkout/provisioning and the requirement for a Core runtime release. Cargo
   obtains the pinned source. If keeping a checkout solely to run Core's optional
   media tests, pin it to the same source revision and pass explicit test paths;
   it must not supply packaging scripts or impose host gates on Core. Source
   `AVID_CORE_LICENSE.txt` from a pinned host-maintained notice copy or the resolved
   dependency source instead of assuming `../AVID Core/LICENSE`. Update
   README, building/architecture/releasing/runtime migration documents and notices.

7. Run `cargo fmt --all -- --check`, `cargo check --workspace --locked`,
   `cargo test --workspace --locked`, and `cargo clippy --workspace --locked -- -D warnings`.
   Run ATIV's revised ownership/package/integration tests. Against the exact
   chosen binaries run Core's five real-media tests using `AVID_TEST_FFMPEG` and
   `AVID_TEST_FFPROBE` (see integration.md). Exercise presets, previews, flips,
   Simple and PerFrame rendering, progress/cancel, input/output aliases, existing
   output preservation, missing/corrupt/mismatched tools and unsupported encoders.
   Build and qualify ATIV packages on each supported OS/architecture, including
   signing/notarization, updater and UI/accessibility checks under ATIV ownership.

Acceptance: a clean ATIV checkout builds with Cargo's pinned dependency without
an adjacent Core repository or Core runtime assets. Packaged media resolves only
the ATIV-supplied pair. A Core source update does not require an FFmpeg rebuild
unless ATIV chooses one. ATIV acceptance does not block a Core source release.
