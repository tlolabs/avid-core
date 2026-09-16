# Changelog

## 0.2.1

Software encoding is authoritative on all six targets. Windows/Linux GPU dependencies and parity gates are removed; macOS VideoToolbox is optional. Minimum-OS qualification remains required. New Video settings default to software while saved choices remain readable. Current scope supersedes the historical hardware requirements below.

This release adds explicit separation of managed runtime executables and metadata for macOS bundles, strengthens runtime acquisition and payload validation, fixes native Windows executable build targets, and makes macOS executable identities deterministic for repeat-build checks.

The FFmpeg recipe remains a candidate. Core source CI has passed; the six-target source-runtime build is still in progress at tagging. Minimum-OS, toolchain and production host qualification remain incomplete. This source tag does not promote or publish FFmpeg runtime binaries. See [current qualification evidence](docs/ffmpeg/software-qualification.md).

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
