# Core 0.3.0 refactor and verification

## Repository inspection and scope

Read-only inspection preceded edits:

| Repository | Inspected HEAD | Relevant state |
|---|---|---|
| AVID Core | `eb57c90` | 0.2.2 development branch; shared renderer plus managed runtime manifests, acquisition, qualification and release workflow |
| ATIV | `f38c433076c2980ff356da3d54e6f9b465439e6d` | Shared renderer already used; sibling path/build.rs pin, managed-layout adapter, packaging coupled to Core Python helpers |
| EnCAP | `860c408d424979aefc84a26adcfec6b3052ccd8e` | Git-pinned 0.2.1; shared Video/state APIs, opt-in managed-runtime feature, sibling helper checks, existing host-owned normal packaging inputs |

Only AVID Core files were changed. Host production migration/qualification was
not performed. The prior runtime migration had not produced a working shared
production-distribution contract, so the new source contract deliberately removes
that coupling instead of preserving deprecated acquisition compatibility.

## Changes

- Added explicit `MediaTools::from_paths` and `from_paths_with_timeout`.
- Kept renderer, commands, validation, metadata/settings/state, presets, timeline,
  encoder detection, errors, staging, progress and process lifecycle behavior.
- Kept optional bounded filesystem helpers for caller-owned runtime replacement.
- Removed managed-directory/layout construction, embedded runtime specification
  and artifact-name mapping from the Rust API.
- Removed executable Core acquisition, host staging/signing receipt, release
  manifest, promotion, matrix retrieval and archive qualification entry points.
  Their original source/tests survive as non-executable historical text.
- Removed binary publishing jobs/permissions and host-packaging release gates.
  Build/compiler/runtime investigations are manual diagnostic work only.
- Retained research recipes, baselines, evidence, build/validation tools and
  compiler fixtures, explicitly marked historical/diagnostic and excluded from
  the Cargo source package. Source releases have no Core → host → Core dependency.
- Added independent system-FFmpeg CI, source-version policy, public integration
  contract and repository-specific migration instructions.

## Local verification (macOS ARM64)

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | Passed |
| `cargo check --locked --all-targets --all-features` | Passed |
| `cargo test --locked --all-targets` | 43 passed; 7 opt-in media/lifecycle tests then run separately |
| `cargo test --locked --all-targets --all-features` | 43 passed; same 7 opt-in tests |
| `cargo test --locked --doc` | 1 passed |
| `cargo package --locked --allow-dirty --offline` | Standalone source archive built and verified; no runtime/scripts included |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps --all-features` | Passed |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | Passed |
| `cargo +1.85.0 check --locked --all-targets --all-features` | Passed |
| `python3 -m unittest discover -s scripts/ffmpeg -p 'test_*.py'` | 13 retained research tests passed |
| `cargo test --locked --test ffmpeg -- --ignored --test-threads=1` with both explicit test paths | 5 passed |
| `cargo test --locked --test runtime_contract -- --ignored --test-threads=1` with external runtime directory | 2 passed |

The real-media pair was read from ATIV's existing `build/ffmpeg-macos-arm64`
folder. Both identify as `9.0.1-https://www.martin-riedl.de`, a host-supplied vendor
build, not a Core release. No host files were edited. Runtime lifecycle tests copy
only the two executables into temporary directories and pass without any Core
manifests. The installed system pair was unsuitable for a positive test because
its FFmpeg/ffprobe identifiers differ (`7.0.2-tessus` vs `7.0.1-tessus`).

The five new deterministic tests cover independent paths/custom executable names,
Unicode/spaces, probe/capabilities/preview/render, ignored historical metadata,
missing executables without fallback, non-Core version identifiers, mismatch and
malformed identity rejection, pre-cancellation and configurable validation timeout.
Existing cancellation/fallback/publication tests now use explicit paths too.

Windows-only file-lock tests cannot execute on macOS. The shared-core CI matrix
retains native Windows/Linux/macOS tests; a separate Linux job runs the seven
opt-in tests using a system-installed pair. Those are library checks, not minimum-OS,
GPU or signed application qualification. No FFmpeg build experiment or runtime
release is required or performed for this refactor.

## Consumption and remaining host work

Recommend source tag `v0.3.0`, with exact package version `=0.3.0`, to both hosts.
Use its full commit as the Cargo `rev` for maximum pin explicitness, and commit
Cargo.lock. The [integration contract](integration.md) defines the API/version
policy and all breaking removals. Follow the exact [ATIV handoff](handoff-ativ.md)
or [EnCAP handoff](handoff-encap.md); they identify adapters, features, scripts,
workflow entries and qualification work that remain in the respective host repo.
