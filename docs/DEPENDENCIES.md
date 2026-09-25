# Dependencies

`Cargo.lock` is the authoritative version pin. [The machine-readable inventory](dependency-inventory.json) lists the complete resolved graph, exact checksums and package license expressions; `script/dependency_inventory.py --check` detects drift.

| Direct dependency | Purpose | License and source |
| --- | --- | --- |
| `serde` | Stable shared settings and state serialization | MIT or Apache-2.0, [crates.io](https://crates.io/crates/serde) |
| `serde_json` | JSON state compatibility | MIT or Apache-2.0, [crates.io](https://crates.io/crates/serde_json) |
| `tempfile` | Staged output and safe temporary-file lifecycle | MIT or Apache-2.0, [crates.io](https://crates.io/crates/tempfile) |
| `same-file` | Detect overlapping source/output paths | Unlicense or MIT, [crates.io](https://crates.io/crates/same-file) |

Target-specific transitive crates include Microsoft's open-source `windows-sys`/`windows-link` bindings (MIT or Apache-2.0). They are distinct from proprietary Windows App SDK redistributables. [Third-party notices](../THIRD_PARTY_NOTICES.md) and [licensing review](../LICENSING_REVIEW.md) explain the distribution boundary.

FFmpeg and ffprobe are caller-supplied separate executables, not crate dependencies or AVID Core release assets. Their exact builds and licenses belong to each consuming application. Rust, GitHub Actions, Python and historical FFmpeg recipe tools are development/build inputs, not linked library code.
