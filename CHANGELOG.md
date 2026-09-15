# Changelog

## 0.2.0

AVID Core now owns the shared FFmpeg source/build specification and runtime compatibility contract for ATIV and EnCAP.

### Added

- An immutable official FFmpeg source pin, external dependency pins and source-build scripts.
- A six-target CI matrix, capability and functional validation, corresponding-source packages, checksums and gated attested publication.
- The opt-in managed-runtime API, artifact naming and embedded specification, with explicit paths and no PATH fallback.
- Cached-frame simple video export, preserving the existing per-frame mode.
- Repository/binary audits, licensing documentation, compatibility fixtures and ordered host migration instructions.

### Compatibility and qualification

Existing discovery and encoding behavior remains available during migration. No host application has been migrated by this Core release.

The FFmpeg specification remains a candidate. Local macOS ARM64 source builds passed compatibility checks and produced byte-identical executables across two clean builds. The remaining platform matrix, hardware dependencies, provenance and host packaging gates are still open. This Core source release does not publish or approve production FFmpeg runtime binaries. See [the verification report](docs/ffmpeg/verification.md) and [migration handoff](docs/ffmpeg/migration.md).
