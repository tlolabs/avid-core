# Testing

Automatic CI is limited to the weekly new-release smoke policy in
[AGENTS.md](../AGENTS.md): native Linux and Windows compilation/basic smoke
checks, with no macOS CI or automatic publication. The former comprehensive
workflows are inactive historical references under
[the workflow archive](../.github/workflow-archive/2026-11-08/README.md).
Do not restore or dispatch them to perform local verification.

The broader diagnostic suite covers formatting, locked builds, unit/integration
tests, documentation tests, Clippy and the Rust 1.85 minimum version. These checks
are nonblocking for ordinary development; deliberate releases still require
applicable functional and integrity verification. Locally, run:

```sh
cargo fmt --all -- --check
cargo check --locked --all-targets
cargo test --locked --all-targets
cargo test --locked --doc
cargo clippy --locked --all-targets --all-features -- -D warnings
python3 script/dependency_inventory.py --check
```

The default suite includes in-process JPEG/PNG optimization tests and needs a C
compiler for the vendored codecs. It does not need FFmpeg for image optimization.
Cover transparent PNG rejection and decoded opaque pixels, lossless JPEG coefficients,
optional JPEG quality, metadata strip/preserve settings, size fallback,
malformed/unsupported images, resource
limits, cancellation, protected paths and failed-publication preservation. Run
the suite natively on each supported platform; Windows/Linux native image-codec
execution is outstanding until actual results are recorded. See the
[image optimization contract](image-optimization.md#verification).

Real-media tests are opt-in and use a caller-provided FFmpeg/ffprobe pair. Set `AVID_TEST_FFMPEG` and `AVID_TEST_FFPROBE`, then run `cargo test --locked --test ffmpeg -- --ignored --test-threads=1`. The runtime contract test uses `AVID_RUNTIME_DIRECTORY`. See [integration verification](integration.md#verification). Windows and Linux CI results establish build/test validation, not personal hands-on application testing by Thomas Lothian; he primarily tests macOS (ARM64).

The performance benchmark requires a runnable local FFmpeg/ffprobe pair and is
opt-in. Run `AVID_RUN_PERF_BENCH=1 cargo bench --bench perf_benchmark` on a host
with that pair. Ordinary `cargo test --all-targets` runs skip the benchmark.

Measured local optimizations, controls and limitations are recorded in
[the performance audit](performance-audit-2026-11-10.md).
