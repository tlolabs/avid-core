# Code signing and provenance policy

AVID Core development and source publication do not require cryptographic commit or tag signing, Apple Developer ID signing, notarization, Windows Authenticode, Linux GPG signatures, or manual approval. DCO trailers are encouraged but are not enforced. The source release workflow creates GitHub build provenance on a best-effort basis; failure to create that optional attestation does not block an otherwise verified source release.

Source archives still undergo compilation, tests, packaging, checksum verification, and dependency-inventory checks before publication. FFmpeg upstream release and tag signatures are verified when acquiring pinned source; that is source-authenticity verification, not distribution signing, and remains part of the build's supply-chain checks.

The Core runtime release remains a separate authenticated update mechanism. Its complete six-target manifest, artifact hashes, native validation, GitHub workflow attestation, and consumer-side attestation verification are integrity controls. They are not code-signing or approval gates and must not be weakened without updating and validating every consumer. Runtime assets are never replaced under an existing release identity. Host applications remain responsible for any signing or notarization required by their own distribution channels.
