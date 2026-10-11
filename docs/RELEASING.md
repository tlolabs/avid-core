# Releasing

AVID Core is a source library. GitHub Releases are the canonical source release record. Stable releases use a `vMAJOR.MINOR.PATCH` tag matching `Cargo.toml`; cryptographic tag signing and manual release approval are not required. Do not invent or silently change versions.

The [source release workflow](../.github/workflows/source-release.yml) is triggered by version tags. It verifies the tag/version identity, builds and tests the package, produces the CycloneDX SBOM and checksums, and publishes only after the artifact passes those checks. GitHub build provenance is best-effort and does not block publication. Failed functional or integrity checks still prevent publication of that source artifact, but do not block unrelated development or other platforms.

The [Core runtime workflow](../.github/workflows/runtime-release.yml) is a separate authenticated update path. It currently publishes one complete six-target runtime manifest; each platform's native artifact is independently built and validated, but publication remains tied to the complete manifest, checksums and GitHub attestation consumed by existing clients. Do not publish a partial runtime or bypass these integrity checks without first changing and validating all consumers. Existing release identities and assets are not overwritten.

For either channel, keep versioning and release notes accurate, update [CHANGELOG.md](../CHANGELOG.md) where appropriate, and maintain dependency/license records when they change. See [TESTING.md](TESTING.md) and [CODE_SIGNING_POLICY.md](../CODE_SIGNING_POLICY.md) for the nonblocking development and provenance policies.
