# Testing

CI runs formatting, locked builds, unit/integration tests, documentation tests and Clippy on macOS, Windows and Linux, plus an Rust 1.85 minimum-version build. Locally, run:

```sh
cargo fmt --all -- --check
cargo check --locked --all-targets
cargo test --locked --all-targets
cargo test --locked --doc
cargo clippy --locked --all-targets --all-features -- -D warnings
python3 script/dependency_inventory.py --check
```

Real-media tests are opt-in and use a caller-provided FFmpeg/ffprobe pair. Set `AVID_TEST_FFMPEG` and `AVID_TEST_FFPROBE`, then run `cargo test --locked --test ffmpeg -- --ignored --test-threads=1`. The runtime contract test uses `AVID_RUNTIME_DIRECTORY`. See [integration verification](integration.md#verification). Windows and Linux CI results establish build/test validation, not personal hands-on application testing by Thomas Lothian; he primarily tests macOS (ARM64).
