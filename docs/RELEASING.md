# Releasing

AVID Core is a source library. GitHub Releases are the canonical source release record. Stable releases use a `vMAJOR.MINOR.PATCH` tag matching `Cargo.toml`; cryptographic tag signing and manual release approval are not required. Do not invent or silently change versions.

Release publication is deliberate. The former source and runtime workflows are
inactive under [the workflow archive](../.github/workflow-archive/2026-11-08/README.md).
Pushing a tag does not publish a release automatically. Do not restore or dispatch
archived workflows without a new explicit owner instruction.

For a source release, verify that the tag matches `Cargo.toml`, build and test the
source, push the release commit and annotated tag, and create its GitHub Release.
GitHub provides the tagged source archives. If publishing a `.crate` asset, package
and verify an exact committed source copy so unrelated untracked files cannot
enter the package. The retained `script/dependency_inventory.py --sbom PATH` tool
can produce a CycloneDX SBOM; verify checksums for any uploaded assets. Failed
functional or integrity checks prevent publication of the affected artifact,
without blocking unrelated development or other platforms. GitHub build
provenance is best-effort, and tag signing is not required.

For 0.4 and later, the complete workspace is the source integration package:
Core depends on local bridge crates. GitHub's tagged source archives include
those crates. Do not offer a standalone `avid-core` `.crate` as a complete
package unless all of its workspace dependencies are separately available and
the packaged crate has been verified against them. Any attached workspace
archive must match the tagged commit and pass its own build and integrity checks.

The authenticated Core runtime is a separate publication contract: one complete
six-target runtime manifest, checksums, and GitHub attestation consumed by existing
clients. Do not publish a partial runtime or bypass these integrity checks.
Existing release identities and assets are not overwritten.

Verified FFmpeg/ffprobe direct downloads remain independent, immutable platform
releases. A platform publisher must verify its own native runtime, media,
lifecycle, repeatability, provenance, archives and target identity before writing
release assets. These releases do not replace authenticated runtime updates.
ATIV and EnCAP remain pinned to `ffmpeg-9.0.1-r7.1` and its manifest, digest, archive
hashes and attestation; see [the provider contract](ffmpeg/provider-contract.md).

Core's native checks qualify the runtime pair for its target. Application
integration, signing, packaging and release schedules belong to consumers and
are not inputs to Core publication. Weekly Linux/Windows smoke CI is diagnostic,
runs only for unattempted stable source tags, and never publishes releases.

For either channel, keep versioning and release notes accurate, update [CHANGELOG.md](../CHANGELOG.md) where appropriate, and maintain dependency/license records when they change. See [TESTING.md](TESTING.md) and [CODE_SIGNING_POLICY.md](../CODE_SIGNING_POLICY.md) for the nonblocking development and provenance policies.
