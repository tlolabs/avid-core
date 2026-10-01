# Code signing policy

The repository owner approved AVID Core for release and ATIV/EnCAP integration on September 30, 2026, based on its passing native builds. The owner assumes application testing after integration. The executable record is `runtime/ffmpeg/qualification.json` under `release_acceptance`.

Cryptographic commit signing and release-tag signing are optional. No private signing key, Apple notarization, Windows Authenticode, Linux GPG setup, application upgrade acceptance, or manual UI review is a prerequisite for building, publishing, or consuming Core. DCO attribution uses a text trailer and does not require a signing key.

Versioned assets retain their original build identity, checksums, source materials and complete six-target runtime matrix. GitHub records promotion provenance using its built-in workflow identity. Existing release tags and assets are never replaced. Platform signing of a public end-user application is a host responsibility and does not block Core use or the owner's test builds.
