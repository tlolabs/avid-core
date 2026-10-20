# Changelog

All notable changes to AVID Core are recorded here. Versions follow [Semantic Versioning](https://semver.org/). Earlier details remain in Git history and GitHub Releases.

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
