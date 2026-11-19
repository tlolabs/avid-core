# Integration contract

For new 0.4 integrations use the [linked Rust/C ABI migration](linked-api.md),
[linked artifact packaging](linked-artifacts.md), and [transcription migration](linked-transcription.md).
Use target-specific verified libraries and all listed native dependencies.
No production operation should launch ffmpeg/ffprobe, Python or a helper.

## Versioning and migration

0.4 is a pre-1.0 minor API break. `Renderer::new()` replaces
`Renderer::new(MediaTools)` explicitly. `LegacyRenderer::new(MediaTools)` is
available only through `legacy-subprocess` for historical comparison. Runtime
pair verification, discovery and management APIs remain behind that feature;
existing authenticated manifests and consumer pins retain their exact contract.
The source release is tagged [`v0.4.2`](https://github.com/tlolabs/avid-core/tree/v0.4.2). Pin that tag's immutable commit and
artifact identities only after native qualification for the host's target.
Use the complete source workspace: the Core crate now depends on local bridge
crates, so a standalone Core `.crate` is not a complete integration package.

Settings and state serialization retain their existing defaults, unknown-field
preservation, schema validation and ordered ID-selection behavior. An explicit
saved software encoding choice remains explicit. `Renderer::default_settings()`
provides a working build-profile default for new requests; the Apple LGPL
profile requires Automatic/Hardware because it omits x264/x265.

## Media and lifecycle behavior

Use a dedicated worker, clone the cancellation token for Stop, copy progress to
the UI dispatcher, and join before teardown. Concurrent independent operations
own independent native contexts. Serialize output writers. Preserve the complete
source/project protected-path list. Core publishes only after successful native
cleanup, nonempty validation and cancellation checks. Complete follows publication.
Cooperative deadlines cannot interrupt every codec/driver call; they are not a
hard host shutdown guarantee. Native errors preserve libav error codes rather
than fake process statuses.

Mobile hosts obtain permissions and import accessible local files. Security
scoped bookmarks and Android content URIs are not Core paths. No implicit model
or executable downloads occur during inference/rendering. Use deliberate verified
model installation and hold leases through model load/infer/unload.

## Verification

Run the [native test instructions](TESTING.md) on every target being shipped.
Build, simulator, physical-device execution and host migration are separate
claims in the [matrix](linked-status.md). Core's checks remain diagnostic and
nonblocking for development; publication requires an artifact that actually passes
its applicable verification. This migration does not authorize publication.
