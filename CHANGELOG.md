# Changelog

## 0.3.0

Core is now a shared Rust source library; hosts own FFmpeg/ffprobe distribution and production qualification.

- Added `MediaTools::from_paths` and `from_paths_with_timeout`, requiring no recipe, manifest, target mapping or Core binary release.
- Preserved shared render/probe/preview, timeline/state, capability, cancellation, progress, error and output-publication APIs, plus filesystem lifecycle helpers.
- Breaking: removed `MediaTools::from_managed_directory`, `from_managed_layout`, `FFMPEG_RUNTIME_SPECIFICATION` and `managed_runtime_artifact_name`.
- Removed active binary acquisition/staging/promotion/publishing helpers and host qualification gates; retained non-executable historical snapshots and manual build/test research.
- Added external-path tests and independent system-FFmpeg CI. Core source tags have no host packaging, signing, updater or UI prerequisites.
- Minimum Rust remains 1.85; GPL-3.0-only remains unchanged. Version identifiers must still match between the supplied executables.

See [integration](docs/integration.md), [ATIV](docs/handoff-ativ.md), [EnCAP](docs/handoff-encap.md), and [verification](docs/shared-library-refactor.md).

Earlier entries below describe the discontinued runtime-distribution architecture and are historical. The unreleased 0.2.2 branch contributed the lifecycle fixes retained in 0.3.0.

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
