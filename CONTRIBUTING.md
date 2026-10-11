# Contributing

Changes may be pushed directly to `main` or submitted through a pull request; neither pull requests nor reviews are required. Open an issue for significant API changes when useful. Keep changes scoped to the shared library; host application packaging and UI behavior belong in ATIV or EnCAP. Run the checks in [docs/TESTING.md](docs/TESTING.md) when practical and update relevant documentation and licensing notices.

Contributions must be legally compatible with GPL-3.0-or-later. A `Signed-off-by:` trailer is encouraged as a development convention to certify the [Developer Certificate of Origin 1.1](https://developercertificate.org/), but it is not required or enforced. Cryptographic commit signing is optional; no signing key is required to contribute or release. Do not add vendored or copied code without clear origin and license information.

Automated tests, builds, security scans and compatibility checks are diagnostic: failures stay visible and tracked but do not block commits, pushes, merges or unrelated development. Release automation remains fail-closed for the artifact being published and publishes only artifacts that pass the applicable functional and integrity checks. No manual review or approval is required for operations under repository control.
