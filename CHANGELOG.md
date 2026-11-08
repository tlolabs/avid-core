# Changelog

All notable changes to AVID Core are recorded here. Versions follow [Semantic Versioning](https://semver.org/). Earlier details remain in Git history and GitHub Releases.

## 0.3.2 - 2026-11-08

### Added

- Source-built, exactly pinned JPEG/PNG optimization with lossless defaults, ImageOptim-style serializable settings for both hosts, metadata controls, opaque album-artwork validation and safe staged output.
- Separate Apple transcription packages with verified upstream model downloads,
  WhisperKit, Parakeet and MLX adapters, word timings, and caption export.

### Changed

- Exclude local audit evidence from source packages.
- Pin local builds and weekly native smoke compilation to Rust 1.99.1.

### Fixed

- Normalize composed artwork to square pixels using verified FFmpeg capabilities,
  fixing portrait timeline concatenation across different source aspect ratios.
- Select GIF/APNG demuxer options by content, preserve PerFrame animation timing,
  and keep Simple exports static; avoid APNG seek-loop decoder hangs and hold the
  last frame after finite APNG playback.

- Reject source/protected destination aliases through missing directories, `..`
  and symlinks, and repeat image output protection after creating the stage.
- Reject malformed PNG palette/transparency data before opacity verification.
- Reject corrupt PNG ancillary checksums before metadata stripping can discard them.
- Strip JPEG trailers and avoid injecting JFIF metadata into EXIF-only images.
- Parse padded FFmpeg progress values and reset fields between progress blocks.
- Invalidate Unix probe caches after same-size replacements with preserved
  timestamps; bypass the cache on platforms without reliable replacement identity.
- Join both tool-validation workers before returning errors and return worker
  startup failures as errors.

## 0.3.1 - 2026-10-20

### Changed

- Declare AVID Core-owned source GPL-3.0-or-later, with the EnCAP-derived rights basis documented in the licensing review.
- Establish source-library documentation, dependency inventory and release policy.
- Bound timeline artwork probing to four workers per render and translate worker panics and thread-start failures into errors.
- Close staged output handles before atomic publication and retry transient Windows sharing and lock errors.
- Pass platform publisher inputs through quoted environment variables rather than interpolating them into shell source.
- Exclude release tooling and generated runtime release plans from the published crate package.

## 0.3.0

### Changed

- Refactor the shared media engine into a host-supplied runtime source library for ATIV and EnCAP.
