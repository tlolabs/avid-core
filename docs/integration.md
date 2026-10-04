# Public integration contract

Core owns reusable Rust media behavior: input/settings validation, presets and
media models, audio-duration/image probes, encoder detection, command/filter
construction, single-image and timeline rendering, PNG preview, progress, child
process cancellation/timeouts, structured errors and staged output publication.
The public surface is re-exported from `src/lib.rs`; command construction and the
process runner remain private implementation details. Existing renderer, model,
state and error APIs are retained. Do not duplicate their implementations in hosts.

Core owns the pinned FFmpeg/FFprobe source build, version, provenance, candidate
packaging and native media qualification. Hosts acquire and install the matched
Core package, verify its external authenticity, and handle application signing,
updates, playback, UI and application lifecycle. A Core candidate is not a
production release until its native and release gates pass.

## Load one Core runtime

```rust,no_run
use avid_core::{CancellationToken, MediaTools, Renderer, Result};
use std::path::Path;

fn renderer(runtime: &Path, token: &CancellationToken) -> Result<Renderer> {
    Ok(Renderer::new(MediaTools::from_core_directory(runtime, token)?))
}
```

The host passes the installed Core runtime directory. Core verifies its pinned
specification, build/source and test evidence, checksums and both executables.
It returns both paths from that one directory, along with `core_runtime()` build
identity. A missing or changed FFprobe fails the entire load. There is no PATH,
bundle or system fallback. Internal hashes detect corruption after acquisition;
the host must verify the archive's external authenticity before installation.

For compatibility, `from_paths` still accepts an explicitly supplied pair and
runs each executable with `-version`, with a 30-second per-process
timeout. `from_paths_with_timeout(ffmpeg, ffprobe, duration, token)` lets the host
choose that timeout. `ffmpeg()`, `ffprobe()`, `ffmpeg_version()` and
`ffprobe_version()` expose the resolved paths and first version lines. Both
identifiers (third whitespace-delimited field) must match, including vendor
suffixes. This is a consistency check, not proof of identical build options or
trust. The Core directory constructor enforces the pinned FFmpeg release and
both validated binary hashes.

`ToolDiscovery` / `MediaTools::discover` remain development convenience APIs,
with conventional adjacent/bundle paths and optional PATH. New production
integrations should use `from_core_directory`.

## Media and lifecycle behavior

Construct `Renderer::new(tools)` and optionally call `with_options(OperationOptions)`.
Use `probe_audio_duration`, `inspect_image`, `capabilities`, `preview`, `render`
or `render_with_mode`. Capabilities report advertised encoders, not proof that a
GPU is usable. There is no blanket guarantee that every matching version supports
every operation. Software H.264 needs libx264/AAC/MP4 and the image/filter pipeline;
HEVC additionally needs libx265. Sequence and Simple-mode filter requirements are
listed in the README. Hosts run media tests against their chosen build and its
required formats; unrelated EnCAP Audio/Transcript codecs remain host requirements.

All operations block: run them on an application worker, never the UI thread.
Clone `CancellationToken` into the host stop handler; callbacks use the operation's
worker and must return promptly. Progress fraction 1 is not success: wait for the
return value and `Stage::Complete`. Core kills/reaps its owned child and drains
pipes before returning. Hosts must finish/cancel work and join workers before
replacing tools. Optional `move_runtime_directory` / `remove_runtime_directory`
retain bounded Windows sharing-lock retries; they do not acquire, install,
authenticate or qualify a runtime and do not implement a host updater.

`Error` and `ProcessFailure` keep structured categories, arguments, exit status
and bounded diagnostics. Hosts translate errors and control logging/privacy.
Protect additional input/project paths with `protected_paths`. Core stages output
next to the destination and publishes only on success. Hosts serialize writes to
the same destination. `VideoSettings` / `VideoProjectState` preserve existing JSON
and unknown fields; validate the schema before execution.

## Versioning and reproducibility

The crate follows semantic versioning. While pre-1.0, breaking public API changes
increment the minor version; compatible fixes increment the patch version.
After 1.0, breaking changes increment the major version. Application versions and
FFmpeg versions are independent. Rust 1.85 and edition 2021 remain the baseline.
Source releases use immutable `vMAJOR.MINOR.PATCH` tags; do not move an existing tag.
A source tag never attests a host package or an FFmpeg runtime.

For both host workspace roots:

```toml
[workspace.dependencies]
avid-core = { git = "https://github.com/tlolabs/avid-core.git", tag = "v0.3.0", version = "=0.3.0" }
```

Keep `avid-core.workspace = true` in consuming members. Alternatively replace
`tag = "v0.3.0"` with `rev = "<full 40-character v0.3.0 commit>"`; resolve it from
an authenticated checkout using `git rev-parse 'v0.3.0^{commit}'`. Never specify
both tag and rev. Commit Cargo.toml and the regenerated Cargo.lock together.
Use `cargo update -p avid-core` to update the lockfile deliberately, then
`cargo test --workspace --locked`. Do not use a sibling path or moving branch in
release builds. No build-time helper checkout is required. Include the Core
license in host distribution through host-owned notice handling.

## Breaking changes from 0.2.x

The 0.3.0 release removed the earlier managed layout APIs. The current Core
directory API restores a strict, complete package contract while keeping
`from_paths` for existing consumers. `FFMPEG_RUNTIME_SPECIFICATION` is embedded
again for version and recipe identity.
Scripts `acquire.py`, `host.py`, `package.py`, `release_manifest.py`,
`retrieve_candidates.py` and `qualify_archive.py` are no longer active APIs.
Historical snapshots are under `docs/ffmpeg/historical/` as `.txt` files.

No render/settings/state/error/progress API was removed. Well-formed version
lines remain compatible; malformed identity prefixes are now rejected explicitly.
The ATIV and EnCAP handoffs list the exact remaining adapters/build scripts to fix.

## Verification

Default, manifest-free suite:

```sh
cargo fmt --all -- --check
cargo check --locked --all-targets --all-features
cargo test --locked --all-targets
cargo test --locked --all-targets --all-features
cargo test --locked --doc
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo +1.85.0 check --locked --all-targets --all-features
python3 -m unittest discover -s scripts/ffmpeg -p 'test_*.py'
```

Optional real media, explicitly against an application-selected pair:

```sh
AVID_TEST_FFMPEG=/absolute/path/to/ffmpeg \
AVID_TEST_FFPROBE=/absolute/path/to/ffprobe \
  cargo test --locked --test ffmpeg -- --ignored --test-threads=1
```

Those environment variables are test-harness inputs, not library discovery policy.
Without either, the real-media suite uses development discovery. Set both or neither.

```sh
AVID_RUNTIME_DIRECTORY=/absolute/path/to/relocatable-pair \
  cargo test --locked --test runtime_contract -- --ignored --test-threads=1
```

The directory test copies only ffmpeg and ffprobe (with `.exe` on Windows), then
checks render/cancel/timeout and release of handles for replacement and rollback.
Use a self-contained or otherwise relocatable pair; adjacent shared-library
packaging must be tested by the host. It needs no `spec.json` or `build.json`.
Core CI also exercises a system-installed FFmpeg pair on Linux. These tests do
not replace each application's production packaging qualification.

For a Core-built candidate, run the separate packaged-pair test after native
build and packaging:

```sh
AVID_RUNTIME_DIRECTORY=/absolute/path/to/core-runtime-directory \
  cargo test --locked --test core_runtime -- --ignored --test-threads=1
```
