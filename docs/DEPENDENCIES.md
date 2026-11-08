# Dependencies

`Cargo.lock` is the authoritative version pin. [The machine-readable inventory](dependency-inventory.json) lists the complete resolved graph, exact checksums and package license expressions; `script/dependency_inventory.py --check` detects drift.

| Direct dependency | Purpose | License and source |
| --- | --- | --- |
| `serde` | Stable shared settings and state serialization | MIT or Apache-2.0, [crates.io](https://crates.io/crates/serde) |
| `serde_json` | JSON state compatibility | MIT or Apache-2.0, [crates.io](https://crates.io/crates/serde_json) |
| `tempfile` | Staged output and safe temporary-file lifecycle | MIT or Apache-2.0, [crates.io](https://crates.io/crates/tempfile) |
| `same-file` | Detect overlapping source/output paths | Unlicense or MIT, [crates.io](https://crates.io/crates/same-file) |
| `sha2` | Authenticate Core runtime content | MIT or Apache-2.0, [crates.io](https://crates.io/crates/sha2) |
| `turbojpeg =1.5.1` | Safe interface to lossless JPEG coefficient optimization and optional lossy encoding | Unlicense or MIT wrapper; native IJG and BSD-3-Clause terms also apply, [upstream](https://github.com/honzasp/rust-turbojpeg) |
| `oxipng =9.1.5` | Lossless PNG optimization | MIT, [upstream](https://github.com/shssoichiro/oxipng) |
| `turbojpeg-sys =1.2.0` | Exact vendored libjpeg-turbo 3.1.0 source pin | Unlicense or MIT wrapper; native IJG and BSD-3-Clause terms also apply, [upstream](https://github.com/honzasp/rust-turbojpeg) |
| `libdeflate-sys =1.26.1` | Exact vendored native source pin for OxiPNG's compression backend | Apache-2.0 wrapper, MIT native source, [upstream](https://github.com/libdeflater/libdeflater) |
| `zopfli =0.8.2` | Source-built PNG DEFLATE compression at Extra effort | Apache-2.0, [upstream](https://github.com/zopfli-rs/zopfli) |
| `png =0.17.16` | Validate PNG pixels and reject transparency before optimization | MIT or Apache-2.0, [upstream](https://github.com/image-rs/image-png) |

TurboJPEG uses its vendored CMake/static build with optional SIMD and a portable
scalar C fallback, without NASM/tool downloads. OxiPNG is used as a library with
Zopfli enabled, without its CLI or parallel feature. The lockfile pins
`turbojpeg-sys` 1.2.0, `libdeflater` 1.26.1 and `libdeflate-sys` 1.26.1. The latter
builds vendored libdeflate C sources; its Rust wrapper is Apache-2.0 and the native
source is MIT. Zopfli 0.8.2 preserves the Rust 1.85 minimum; newer versions must
be evaluated before changing the pin.
Direct native constraints preserve the source versions even when a consuming
application resolves a fresh lockfile.
Builds require CMake and a target C compiler. See [building](BUILDING.md), the
[image API contract](image-optimization.md), and the preserved upstream texts in
[licenses/image-optimization](../licenses/image-optimization/README.md).

Target-specific transitive crates include Microsoft's open-source `windows-sys`/`windows-link` bindings (MIT or Apache-2.0). They are distinct from proprietary Windows App SDK redistributables. [Third-party notices](../THIRD_PARTY_NOTICES.md) and [licensing review](../LICENSING_REVIEW.md) explain the distribution boundary.

FFmpeg and ffprobe are separate executables, not linked Rust crate dependencies.
Core owns their pinned source builds, dependencies, notices, provenance and
verified platform releases. Hosts select the Core runtime and own application
integration and distribution signing. See [the provider contract](ffmpeg/provider-contract.md).
Rust, the target C compiler, GitHub Actions and Python are development/build
inputs; the image codecs are linked library code with their own notices.
