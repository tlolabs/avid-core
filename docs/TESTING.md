# Verification

Use an explicitly built pinned linked FFmpeg SDK. Default production tests never
launch media executables. Native compilation/execution and physical-device
qualification are separate in [the status matrix](linked-status.md).

```sh
export AVID_FFMPEG_PREFIX=/absolute/pinned/target-prefix
export AVID_FFMPEG_STATIC=1
cargo fmt --all -- --check
cargo check --locked --offline --all-targets
cargo test --locked --offline --all-targets
cargo test --locked --offline --doc
cargo clippy --locked --offline --all-targets -- -D warnings
cargo +1.85.0 check --locked --offline --all-targets
python3 script/dependency_inventory.py --check
python3 -m unittest discover -s scripts/linked -p 'test_*.py'
python3 -m unittest discover -s script -p 'test_*.py'
python3 -m unittest discover -s scripts/ffmpeg -p 'test_*.py'
```

For the complete optional linked inference plus historical regression suite, build
the pinned whisper.cpp prefix using `script/build_whisper.py`, set
`AVID_WHISPER_PREFIX`, then run workspace `--all-features --all-targets` checks,
tests and clippy. `legacy-subprocess` is reference coverage only; never ship it.
The existing ignored executable-runtime tests require their explicit runtime/tool
environment; an ignored test is not a native pass.

`tests/linked_media.rs` generates PNG/WAV in process and tests real probes,
capabilities, previews, both compositions/modes, single/timeline exports,
cancellation/deadlines, corrupt/nested inputs, aliases and repeated/concurrent
cleanup. `tests/linked_decoding.rs` streams decoded RGB and mono PCM to check
cadence/counts, geometry/SAR, silence/tone onset, audio formats, hard cuts,
EXIF orientation, GIF/APNG timing and loop/hold/freeze behavior. Comparisons use
decoded colors/audio/time tolerances rather than compressed-byte equality.
Small generated animation/delayed-track fixtures retain source provenance.

For a deliberate decoded comparison with the historical backend, set explicit
absolute `AVID_TEST_FFMPEG` and `AVID_TEST_FFPROBE` paths to the retained reference
pair, then run:

```sh
cargo test --locked --offline --features legacy-subprocess --test linked_reference -- --ignored
```

This checks both compositions/modes, single/timeline exports, flips, previews,
timestamps, square pixels, audio layout and decoded PCM. It allows mean RGB
error below 4/255 and PCM RMSE below 0.02 for encoding/rounding variation, plus
AAC end padding; it does not shift later clips to hide timing regressions.
Only this explicit reference test invokes media executables. It is excluded
without `legacy-subprocess`. Optional `AVID_TEST_REFERENCE_EVIDENCE` names a
directory for preserving failed audio comparison outputs.

For the production renderer's timeline path-normalization microbenchmark, use
the same explicit linked SDK and run:

```sh
cargo test --locked --offline --release -p avid-core --lib benchmark_timeline_path_rewrite -- --ignored --nocapture
```

This compares the former clone/revalidate phase with in-place path rewriting on
1,024 validated clips. Each process alternates nine rounds of 256 iterations;
input setup, file checks, codecs, publication and final destruction are outside
the timing. Run several separate processes after compilation. This measures one
preparation phase and does not establish an end-to-end export speedup. The
default test run intentionally ignores this manual benchmark.

For a deliberate full-resolution artwork retention regression, build a software
H.264 Core shared library and run:

```sh
python3 script/test_linked_memory.py --library /absolute/path/to/libavid_core_ffi.dylib
```

Use `.so` or `.dll` on the corresponding native host. Fresh workers compare
small/large artwork and one/sixteen Simple timeline clips with no media tools on
PATH. The comparison accounts for graph overhead; it detects accumulated decoded
source frames rather than asserting one universal process-memory limit. Windows
measurement support requires native execution before it can be claimed verified.

Compile `examples/c-linked/main.c` against the development `libavid_core_ffi`, and
compile both Swift files under `examples/apple-linked` using the `include` module
map. Use a writable Swift `-module-cache-path` in restricted environments. Test
both hosts with:

```sh
python3 script/test_linked_hosts.py --host /absolute/c-host --host /absolute/swift-host
```

Optional `--ffprobe /absolute/development/ffprobe --ffmpeg /absolute/development/ffmpeg`
independently validates MP4 stream identity and decoding. Those are test verifiers;
the host export runs with an empty PATH and calls only the linked library.
The host must return a structured cancellation and preserve the existing output.
Use `scripts/linked/build_apple_host.py` for the UIKit export/cancel/AVAssetReader
host; follow Android test-host instructions under `examples/android-linked`.
Run playback/decoded checks on each simulator and available physical device.
The Android host writes `files/result.json` and independently decodes MP4 frames
and PCM through MediaCodec with bounded buffers, geometry/color, tone and timing
assertions. A host-JVM JNI check can exercise its transport on macOS; it does not
replace an Android NDK build or emulator/device result.

Real portable inference tests, exact model pin, cache/lease behavior, native
cancellation and timing restrictions are described in
[the transcription evidence](linked-transcription.md). Never infer actual
recognition or forced alignment from contract-only tests. Test memory during
loading/inference as well as long-input export, and qualify simultaneous export
and inference on the hardware being shipped.

All checks are diagnostic and nonblocking for development. Failures and skipped
or unavailable checks remain visible. Scheduled CI stays the weekly stable-tag
Linux/Windows basic smoke path; no archived automation is enabled.
