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

Real portable inference tests, exact model pin, cache/lease behavior, native
cancellation and timing restrictions are described in
[the transcription evidence](linked-transcription.md). Never infer actual
recognition or forced alignment from contract-only tests. Test memory during
loading/inference as well as long-input export, and qualify simultaneous export
and inference on the hardware being shipped.

All checks are diagnostic and nonblocking for development. Failures and skipped
or unavailable checks remain visible. Scheduled CI stays the weekly stable-tag
Linux/Windows basic smoke path; no archived automation is enabled.
