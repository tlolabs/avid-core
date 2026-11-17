# ATIV / EnCAP linked media handoff

This change belongs to AVID Core and its test hosts. Neither application's
source, dependency pins, authenticated update feeds or release assets have been
changed. The `0.4` API is an explicit pre-1.0 minor break; application adoption
and each target's native qualification remain separate work.

## Replace the media client

Use the complete Core workspace and a target-specific candidate produced by
`scripts/linked/build.py`, `build_core.py` and `package.py`. Retain its
`linked-artifact.json`, checksums, corresponding sources and notices. These are
linked-library artifacts, not replacements for the historical authenticated
executable-runtime manifest. Do not update an existing consumer pin merely
because the library compiled on another target.

For Rust clients, replace the executable-dependent constructor explicitly:

```rust,ignore
// Before (0.3)
let renderer = Renderer::new(MediaTools::from_core_directory(runtime, &cancel)?);
// After (0.4)
let renderer = Renderer::new();
```

Keep existing `RenderRequest`, `Input`, `Timeline`, presets and saved
`VideoSettings` / `VideoProjectState` conversion. Seed a new project's settings
from `renderer.default_settings()`; keep saved explicit encoding choices.
Remove executable discovery, version checks and child-process protocols from the
media client. Do not enable `legacy-subprocess` in application builds or catch
linked errors by launching a helper. Audio duration, image inspection,
capabilities, previews, exports and optimization all have direct APIs.

For foreign clients, check `avid_core_abi_version() == 1`, allocate a Core handle
and one cancellation handle per job, then call `avid_core_execute_json` on a
host worker. Both Rust and ABI requests reach the same media policy. ABI
schema examples and complete memory/threading rules are in [linked-api.md](linked-api.md)
and `include/avid_core.h`. For example:

```json
{"schema_version":1,"operation":"render","image":"/local/cover.png","audio":"/local/audio.wav","output":"/local/output.mp4","mode":"per_frame","settings":{"width":1920,"height":1080,"fps":30,"encoding":"automatic","codec":"h264","composition":"fitted","audio_bitrate":"128k"},"protected_paths":["/local/project.avid"]}
```

Copy callback bytes before posting UI updates. Treat a zero handle, malformed
response, or `ok:false` as failure. Preserve native error details and nested
hardware/software failures. Cancel through the token, wait for the synchronous
operation to return, release result/token/Core handles, and only then unload the
library or replace its files. Release does not cancel or join a job. Serialize
writers to each destination and keep original inputs accessible until return.
Only `complete` means published output. Cooperative codec/driver interruption
does not provide a hard shutdown deadline.

## Link each host

| Host | Replacement integration and target boundary | Required native evidence |
| --- | --- | --- |
| macOS Apple Silicon / Intel | Link the matching macOS archive or universal macOS slice of `AVIDCore.xcframework`; import the C module and `examples/apple-linked/AVIDCore.swift`. Retain the frameworks/system dependencies listed in the artifact contract. | C and Swift export/progress/cancel plus MP4 decode on each architecture; cross-building Intel does not establish Intel execution. |
| iPhone / iPad | Link the iOS device ARM64 slice, minimum iOS 16 for media-only builds. Keep device and simulator libraries distinct. | Signed development host export, cancellation and AVAssetReader decoding on actual iPhone/iPad; provisioning is host-owned. |
| iOS simulator | Link ARM64 or x86_64 simulator slices. A profile with software encoders is needed if VideoToolbox is unavailable; an explicit Hardware request still fails. | Run the UIKit host, inspect `Documents/result.json`, and require decoded video/audio counts and cancellation preservation. |
| Windows x86_64 / ARM64 | Link the target C ABI DLL using the header and an import library matching the host linker. Core's recipe uses GNU/LLVM, not MSVC Rust archives. Copy every declared non-system runtime DLL with notices. A Qt adapter copies callbacks into queued signals. | Native architecture/dependency checks, C/C++ host export/progress/cancel and full MP4 decode. |
| Linux x86_64 / ARM64 | Link the target `.so` or static archive and its declared transitive dependencies. Package-relative library lookup must not reference a development prefix. | Native host test and decoded output on each architecture and the oldest supported glibc baseline. |
| Android ARM64 / x86_64 emulator | Link `libavid_core_ffi.so` through the thin JNI transport in `examples/android-linked`; include the exact shared NDK C++ runtime and notices. Use the specification-pinned NDK, API 26 minimum and 16 KB linker flags. | Build JNI/APK, check every ELF and APK alignment, run export/progress/cancel and MediaCodec decode on emulator and device. A 4 KB run does not qualify 16 KB compatibility. |

Build the five distinct Apple candidates from one unchanged source snapshot,
then run `scripts/linked/xcframework.py`; it combines only architectures in the
same platform. Follow [linked-artifacts.md](linked-artifacts.md) for exact build,
package, SDK, NDK, linkage and host commands. Media-only Apple minima are macOS13
and iOS16; optional inference combinations may require newer versions recorded
in `minimum_runtime`. The packager inspects binary deployment minima and rejects
an artifact requiring a newer OS than declared.

Apple permission-scoped URLs/bookmarks require the host to hold access through
completion or import into sandbox-local storage. Android `content://` URIs
require ContentResolver import. Pass accessible filesystem paths, not platform
permission objects or URIs. The ABI uses length-delimited UTF-8 paths, including
Unicode Windows paths. No alternative I/O adapter is provided.

## Retain application contracts

ATIV adapters should keep `Fitted` composition and their saved settings policy.
EnCAP adapters should keep `SquarePadded`, ordered clip selection, per-clip audio
mapping, protected project paths and project archive handling in the host.
Choose either composition through Core; do not reproduce filter graphs in
Swift, Qt, C++ or JNI. Keep host playback, permission UI, lifecycle UI and project
storage responsibilities separate from media execution.

Before adopting an artifact, exercise both compositions, modes, presets,
static/animated artwork, H.264/HEVC where compiled, explicit encoding choices,
Unicode paths, failed exports and application teardown. Test with no
ffmpeg/ffprobe available to the production host. Independent test-only decoding
tools may be supplied explicitly to `script/test_linked_hosts.py`. The opt-in
`tests/linked_reference.rs` compares decoded frames/audio/geometry/timing to the
historical backend; compressed files need not match byte for byte.

The [qualification matrix](linked-status.md) records compilation and execution
separately. Missing hardware/SDKs and incomplete independent review remain
visible. Apple LGPL FFmpeg omits GPL/nonfree FFmpeg components but does not
relicense GPL AVID Core or grant App Store clearance. Distribution rights and
static-link obligations remain the distributor's assessment under the retained
[licenses and notices](../LICENSING_REVIEW.md). A library/test host build does
not publish or notarize an application. Host distributables retain their own
signing/notarization, update authentication and publication requirements.
