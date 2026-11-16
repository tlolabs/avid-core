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

Production media builds now link FFmpeg libraries through the reviewed bridge.
They require an explicitly selected Core-built SDK prefix; Core does not discover
media tools or download code at application startup. Follow the separate
[linked library build/artifact contract](linked-artifacts.md) for all target
triples, the Apple LGPL/no-GPL FFmpeg profile, software codec and Apple hardware+software fallback profiles, source
identity, native dependencies, C ABI packages and XCFramework assembly.

```sh
AVID_FFMPEG_PREFIX=/absolute/path/to/linked/prefix AVID_FFMPEG_STATIC=1 cargo build --locked
```

The legacy executable implementation is behind the explicit `legacy-subprocess`
feature for reference testing. Historical executable artifacts, acquisition
signatures and consumer pins remain unchanged. The image optimizer still builds
its pinned native sources in process. Compilation and actual native execution
are reported separately; cross compilation is not platform qualification.
