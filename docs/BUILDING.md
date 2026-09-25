# Building

AVID Core requires Rust 1.85 or later. From this repository, run `cargo build --locked`, `cargo test --locked --all-targets`, and `cargo package --locked --allow-dirty` when checking source package contents. The crate has no Windows App SDK, WinUI, .NET or application packaging prerequisite.

The examples and opt-in real-media tests need a separately installed matching FFmpeg/ffprobe pair. See [testing](TESTING.md) and [integration](integration.md) for the tool paths and media contract. Host applications own production tool provenance and signing.
