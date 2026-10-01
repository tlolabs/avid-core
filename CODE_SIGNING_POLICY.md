# Code signing policy

AVID Core publishes source code for a Rust library. It does not publish a Windows application, installer, portable ZIP, AppImage, macOS app, or bundled FFmpeg runtime. Application code signing belongs to ATIV and EnCAP. No AVID Core binary is currently submitted to SignPath Foundation.

An official stable AVID Core release is an immutable `vMAJOR.MINOR.PATCH` Git tag intentionally created and cryptographically signed by Thomas Lothian, the sole maintainer and release approver. CI must verify the tag against a configured trusted public key before publishing release metadata or assets. Prerelease and development runs are not production signed. Development build artifacts, if produced, are unsupported and retained for 30 days.

Stable source releases should include a locked dependency SBOM and SHA-256 checksums for any downloadable assets. CI records provenance and tests from the tagged source. Private signing keys and service credentials remain in secure local stores or protected CI secrets, never in Git. Thomas verifies the source revision, tag signature, checksums and release result. A provider-required manual approval applies if a provider is ever used; no extra approval gate is needed merely to repeat signed-tag authorization.

If AVID Core later ships a compiled executable or uses SignPath, this policy and the exact binary licensing inventory must be updated before signing. SignPath's explicit System Libraries exception is evaluated on the actual artifact.
