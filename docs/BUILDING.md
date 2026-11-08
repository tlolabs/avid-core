# Building

AVID Core requires Rust 1.85 or later, CMake and a C compiler for the target platform.
Use Xcode Command Line Tools on macOS, the MSVC C/C++ build tools for the MSVC
Windows target, or a C build toolchain on Linux. Cargo compiles the vendored
libjpeg-turbo and libdeflate sources pinned by `Cargo.lock`; no system ImageOptim,
TurboJPEG or OxiPNG installation is required. SIMD is optional: the JPEG build
can fall back to scalar C when NASM is unavailable. No prebuilt codec or
build-time tool download is used.
Cross-compilation also needs a C compiler, archiver and SDK for the target.

From this repository, run `cargo build --locked`, `cargo test --locked --all-targets`,
and `cargo package --locked --allow-dirty` when checking source package contents.
The crate has no Windows App SDK, WinUI, .NET or application packaging prerequisite.

The video examples and opt-in real-media tests need a separately installed
matching FFmpeg/ffprobe pair. Core owns its source-built runtime and provenance;
hosts select a published Core runtime and own application integration, signing
and packaging. See [testing](TESTING.md), [integration](integration.md) and the
[provider contract](ffmpeg/provider-contract.md). The
[JPEG/PNG optimizer](image-optimization.md) operates in process and needs no
FFmpeg runtime. Its native Windows/Linux codec execution remains to be verified;
a cross-build alone does not establish native operation.
