# FFmpeg/ffprobe provider and consumer contract

AVID Core builds the runtime technology. ATIV, EnCAP, and other applications
integrate it and decide when their own releases are ready. A failure in either
repository does not become a release gate for the other.

## Ownership

| AVID Core owns | Each consuming application owns |
| --- | --- |
| Pinned FFmpeg and ffprobe sources, dependency acquisition, source signature checks, configuration, and native compilation | Selecting a published Core runtime for each application target |
| Matched executable packaging, corresponding sources, licenses, version and provenance records | Authenticating and installing the selected runtime under its own update policy |
| Native target, media, lifecycle, repeat-build, archive, checksum, and integrity checks | Application integration, feature, packaging, compatibility, upgrade, and acceptance tests |
| Independent publication and technical documentation for macOS ARM64/Intel, Windows ARM64/x64, and Linux ARM64/x64 | Application signing, notarization, release approval, schedule, and distribution |

Core does not build ATIV or EnCAP, import their test outcomes as publication
requirements, or certify their application releases. Optional host observations
may be recorded for diagnostics, but absence or failure of such an observation
cannot prevent a Core runtime that passed its own target checks from publishing.
An application test failure remains that application's issue.

## Current publication channels

The [platform workflow](../../.github/workflows/ffmpeg-independent.yml) has six
independent read-only native builds. Each publisher depends only on its own
successful build. The publisher rechecks the exact Actions artifact, matched
runtime and source archives, native target, embedded build identity, and SHA-256
checksums before publishing an immutable GitHub Release. It has Actions read,
contents write, attestation write, and OIDC permission; native builds have
contents read only. No platform code signing,
notarization, manual approval, consumer build, or other platform is required.
The release tag follows
`platform-ffmpeg-VERSION-rRECIPE-TARGET-runRUN_ID`. Each Release contains the
runtime archive, corresponding source archive, checksum files, and
`platform-release.json` with the exact target, build revision, Actions origin,
runtime/source filenames and SHA-256 digests. The publisher attests that file
with `actions/attest` and verifies the signer workflow, source digest, main ref,
and hosted runner restriction expected by ATIV. GitHub stores the attestation
by manifest digest: consumers download the manifest from the Release and resolve
its proof through `gh attestation verify`. The five release assets and checksum
format remain unchanged. The release tag is created at the publisher workflow's
exact source commit, so it matches the attestation source digest. If attestation
is unavailable, verified direct downloads still publish and the publisher job
reports an authentication failure. Such a release is not authenticated for ATIV.
Published assets are never overwritten. Failures remain visible in Actions and
[the consolidated issue](https://github.com/tlolabs/avid-core/issues/9).

The separate [authenticated runtime workflow](../../.github/workflows/runtime-release.yml)
preserves the published `ffmpeg-9.0.1-r7.1` six-target manifest, asset hashes,
and GitHub attestation. [Current acquisition](../../script/core_runtime.py)
requires an exact pinned manifest digest, promotion revision, attestation signer,
tag, and target archive/source hashes. It must never accept an incomplete
manifest under that identity. Per-platform attestations form a separate contract;
they do not convert a direct-download Release into the old six-target format.

The Recipe 7 `spec.json` is embedded in published packages and the Rust runtime
identity check. Its historical `qualification_policy.host_packaging: required`
field is frozen provenance from an older plan, not a live release condition.
The active schema-3 ledger marks host packaging informational, and
`script/core_release.py` rejects a ledger that makes it required. Changing the
embedded specification merely to rewrite that historical field would change
the identity of existing packages and violate the pinned consumer contract.

## Consumer selection and migration

ATIV and EnCAP currently pin `ffmpeg-9.0.1-r7.1`, the complete manifest digest,
the Core build and promotion revisions, and per-target archive hashes. They can
build and release against that already published runtime without waiting for a
new Core platform build. Their workflows and application release decisions are
owned by their repositories. No downstream repository is changed here.

The six-target manifest prevents those current acquirers from selecting a new
single-platform Release as an authenticated update. The target-specific contract
uses the existing `platform-release.json` and its GitHub attestation alongside
the existing six-target contract:

1. Core publishes one target-scoped `platform-release.json` with target, build
   revision/run/job/artifact identity and runtime/source hashes, then attests
   its exact bytes from the per-platform publisher workflow. The immutable tag
   identifies the publisher revision.
2. Each application adds an explicit target-scoped pin and acquirer mode.
   That mode must verify the exact tag, pinned manifest digest, trusted Core
   signer workflow and source revision/ref, target identity, archive/source
   hashes, and embedded runtime checksums before installation. No fallback to
   an unsigned asset, system executable, or arbitrary GitHub release is allowed.
3. Each application would run its own tests with the selected pair, then publish
   on its own schedule. Its existing six-target acquisition path stays available
   throughout migration and for rollback.

Consumers requiring authenticated updates retain the existing pinned release
until they have verified a newly attested platform release and completed their
own integration tests. A direct-download asset can be inspected or downloaded
without cross-platform delay, but its checksum file alone does not establish
external authenticity.
