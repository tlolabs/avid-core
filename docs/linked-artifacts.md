# Linked library build and artifact contract

`runtime/linked/spec.json` is the schema-1 source and target specification for
**linked** Core artifacts. Its identity is `avid-linked-v1-PROFILE-TARGET`, and
its package manifest is `linked-artifact.json`. It is independent of the
historical `ffmpeg-9.0.1-r7.1` executable runtime, signed six-target manifests,
published assets and consumer pins. Those records remain unchanged.

A library build is not native qualification. Package manifests deliberately say
`unqualified` until native execution evidence exists. No script in
`scripts/linked` publishes, pushes, changes CI schedules or installs code during
application execution. Required tools and sources must already be present;
`build.py --fetch` is an explicit developer acquisition operation.

## Profiles and defaults

| Profile | FFmpeg combination | Production default | Restrictions |
| --- | --- | --- | --- |
| `apple-lgpl` | FFmpeg 9.0.1, zlib 1.3.1, VideoToolbox H.264/HEVC and AAC/AudioToolbox; GPL, version3 and nonfree components disabled | `Renderer::default_settings()` selects Automatic | No software H.264/HEVC encoder. Simulator or device hardware initialization may fail. An explicit Software request is unsupported. |
| `apple-gpl` | FFmpeg 9.0.1, zlib, x264/x265 plus explicit VideoToolbox H.264/HEVC and AudioToolbox | Automatic | GPL combination; provides same-codec hardware-to-software fallback. Hardware usability remains device-specific. |
| `gpl-software` | FFmpeg 9.0.1, zlib 1.3.1, x264 `b35605ace3ddf7c1a5d67a2eb553f034aef41d55`, x265 4.1 | Software | GPL codecs; portable scalar x264/x265 builds. No claim of hardware availability. |

Saved `RenderSettings` retain their encoding value; the new renderer does not
reinterpret an explicit Software or Hardware choice. The Apple profile does not
silently substitute another codec when VideoToolbox cannot initialize.
Capabilities distinguish compiled encoders from successful initialization.

All three profiles build avformat, avcodec, avfilter, avutil, swscale and swresample as
static libraries. Programs, devices and networking are disabled. The only
compiled media protocol is `file`. Native library use still enforces the Core
input protocol and nested-reference restrictions. There is no runtime download,
PATH lookup or package-manager fallback.

## Source and dependency identities

FFmpeg's pinned release archive has SHA-256
`cf38e0e28c7e5605942c4a77755349b0145804a397af37eb1fb4c77cb237f635`;
its signed tag `n9.0.1` resolves to
`bf1b838f2ab88b4f8fd83443325c782ea0e0f7fa`. Builds verify the pinned archive
hash, release signature, signed tag fingerprint and complete archive/tag tree
comparison. Cached authenticated inputs permit offline builds. The release and
tag key identities are recorded in the independent specification; no historical
runtime authentication is weakened.

The specification also records exact zlib/x265 archive hashes and the x264 git
export hash. The prefix includes pristine corresponding source archives,
upstream notices, the build scripts, enabled components, toolchain identities,
configuration and per-library hashes in `linked-ffmpeg.json`.

Native image dependencies remain source-built from Cargo's pinned packages:
libjpeg-turbo 3.1.0 at `20ade4dea9589515a69793e447a6c6220b464535`
(`turbojpeg-sys` 1.2.0), native libdeflate 1.26 (`libdeflate-sys` 1.26.1),
OxiPNG 9.1.5 and Zopfli 0.8.2. Cargo supplies the target compiler/SDK to their
CMake/cc builds. The final static archive is inspected member by member, so a
host object in either dependency causes rejection. Compilation does not establish
image-codec execution on that platform. Original notices and source checksums
remain under `licenses/image-optimization` and `Cargo.lock`.

## Build one target

The upstream configure scripts require a work path without spaces. A fresh path
is required for each attempt; failed attempts remain available for diagnosis.
Run from the repository root:

```sh
python3 scripts/linked/build.py --target macos-arm64 --profile apple-lgpl \
  --work /tmp/avid-linked-macos-arm64 --jobs 8
python3 scripts/linked/build_core.py --target macos-arm64 \
  --prefix /tmp/avid-linked-macos-arm64/prefix \
  --target-dir /tmp/avid-core-macos-arm64 \
  --receipt /tmp/avid-core-macos-arm64-build.json
python3 scripts/linked/package.py --target macos-arm64 \
  --prefix /tmp/avid-linked-macos-arm64/prefix \
  --core-library /tmp/avid-core-macos-arm64/aarch64-apple-darwin/release/libavid_core_ffi.a \
  --core-build-receipt /tmp/avid-core-macos-arm64-build.json \
  --output /tmp/avid-linked-packages
```

`--fetch` on the first command deliberately acquires missing pinned build
sources/authentication data. Without it, missing or mismatched inputs are errors.
GPL software builds additionally require CMake **3.31.6**, passed with
`--cmake /path/to/cmake`. x265's pinned source uses CMake policies removed by
CMake 4; the recipe does not silently use a different source release. x265's
cross-generated pkg-config file receives an explicit target C++ runtime
link dependency, avoiding accidental success through a development-machine C++
library. ASM is disabled in both software codec builds; there is no NASM download.

`build_core.py` associates exact Core source hashes, selected features, Rust
version and output hashes in its receipt. Source mutation during the build is an
error. Packaging refuses stale receipts, changed FFmpeg archives, target
mismatches, missing corresponding sources or missing transitive runtime
libraries. `cargo vendor --locked --offline` includes the checksummed Rust source
dependencies. This produces a candidate, with a deterministic archive timestamp
and checksums; it does not assert bit-identical output across different compiler
or SDK versions.

Apple validation checks the actual minimum OS in every Mach-O archive member and
dynamic library against the declared combined deployment minimum. A newer object
cannot be relabeled as supporting an older OS. Desktop C and C++ compiler targets
must agree with the requested OS/ABI as well as architecture. Build receipts
reject omitted source symlinks and new/changed source files during packaging.

A direct development build can set `AVID_FFMPEG_PREFIX` to an absolute, explicitly
chosen SDK and `AVID_FFMPEG_STATIC=1`. Production prefixes must carry
`linked-ffmpeg.json` matching the target, pinned revision and archive hashes.
The prefix retains the exact specification used for its build. Adding another
profile does not relabel an existing SDK: the stored specification hash is
verified and its selected target/profile/dependency recipe must match the
current recipe before reuse. A changed selected recipe requires rebuilding that
SDK.
`AVID_FFMPEG_DEV=1` permits an explicitly selected development SDK without Core
provenance; such a build is not a production artifact. Neither path discovers a
system FFmpeg installation automatically.

### Optional portable inference artifact

Default Core ABI artifacts include media and portable transcript/model contracts.
Build with `build_core.py --whisper` and an explicit `AVID_WHISPER_PREFIX` for the
pinned CPU-only Whisper runtime described in `script/build_whisper.py`. The
receipt verifies its exact source revision, target, metadata hash and all four
native archive hashes. For such a build, `package.py` additionally requires
`--whisper-prefix /exact/prefix --whisper-source-archive /exact/whisper.tar.gz`.
It verifies the unchanged original source archive hash and the separately pinned
`avid-exception-safety-r1` patch and patched-file hashes. The package ships full
whisper.cpp/ggml base source, the patch, its manifest, the build recipe and both
the original MIT and AVID patch GPL-3.0-or-later notices. The modified SDK's
combined license expression is `MIT AND GPL-3.0-or-later`; it is not an unmodified
MIT-only SDK. Older unpatched SDK receipts are rejected. Models are separate deliberately
installed artifacts with separate licenses; no model is bundled. Omitting this
source or supplying another runtime is a packaging error. Inference packages use
a distinct `-whisper` artifact identity, and an XCFramework cannot mix feature
sets across slices.

The combined package's `minimum_runtime` records the greater of the media and
inference deployment minima. The current inference defaults are macOS 14,
iOS 17 and Android API 28, compared with media-only 13/16/26. Core's Apple
compilation target is raised accordingly for a combined build. This is a build
identity, not physical-device inference qualification. Apple-native Swift
providers remain separate linked packages with their own dependencies and model
formats. A Whisper build does not provide Parakeet or Qwen model compatibility.
The supported CPU/non-DTW exception boundary and its fault-test scope are
documented in [linked transcription](linked-transcription.md#native-exception-boundary).

## Target matrix and tools

| Target ID | Rust target | SDK/toolchain and minimum |
| --- | --- | --- |
| `macos-arm64` | `aarch64-apple-darwin` | Xcode macOS SDK, macOS 13.0 |
| `macos-x86_64` | `x86_64-apple-darwin` | Xcode macOS SDK, macOS 13.0 |
| `ios-arm64` | `aarch64-apple-ios` | iPhoneOS SDK, iOS/iPadOS 16.0 |
| `ios-simulator-arm64` | `aarch64-apple-ios-sim` | iPhoneSimulator SDK, iOS 16.0 |
| `ios-simulator-x86_64` | `x86_64-apple-ios` | iPhoneSimulator SDK, iOS 16.0 |
| `windows-x86_64` | `x86_64-pc-windows-gnu` | Explicit LLVM/MinGW x86_64 compiler, Windows 10 target; native qualification pending |
| `windows-arm64` | `aarch64-pc-windows-gnullvm` | Explicit LLVM/MinGW ARM64 compiler, Windows ARM64 target; native qualification pending |
| `linux-x86_64` | `x86_64-unknown-linux-gnu` | Target GCC/sysroot, glibc baseline must be measured on native host |
| `linux-arm64` | `aarch64-unknown-linux-gnu` | Target GCC/sysroot, glibc baseline must be measured on native host |
| `android-arm64` | `aarch64-linux-android` | NDK 28.2.13676358, API 26, `arm64-v8a` |
| `android-x86_64` | `x86_64-linux-android` | NDK 28.2.13676358, API 26, `x86_64` emulator |

The current Windows recipe deliberately identifies the GNU/LLVM ABI. It does
not claim that an MSVC `.lib` is interchangeable. C ABI DLL consumers can use an
appropriate import library; desktop C++ hosts keep their own C++ ABI across the
C boundary. Cross compilers, Rust target standard libraries and SDK/NDK are
prerequisites, not downloaded by a running application. Linux's requested
baseline is not a native compatibility pass: verify symbol versions and execute
on the oldest supported distribution before claiming that baseline.

## Apple packaging and host test

Create each of the five Apple target packages from one unchanged source snapshot.
`xcframework.py` checks checksums, ABI/profile/source equality, complete supported
architectures and platform identities. It uses `lipo` only within macOS or within
the simulator platform. Device ARM64, simulator ARM64 and macOS ARM64 are never
combined into the same fat archive:

```sh
python3 scripts/linked/xcframework.py \
  --slice /tmp/avid-linked-packages/avid-linked-v1-apple-lgpl-macos-arm64 \
  --slice /tmp/avid-linked-packages/avid-linked-v1-apple-lgpl-macos-x86_64 \
  --slice /tmp/avid-linked-packages/avid-linked-v1-apple-lgpl-ios-arm64 \
  --slice /tmp/avid-linked-packages/avid-linked-v1-apple-lgpl-ios-simulator-arm64 \
  --slice /tmp/avid-linked-packages/avid-linked-v1-apple-lgpl-ios-simulator-x86_64 \
  --output /tmp/AVIDCore.xcframework
```

Headers and `module.modulemap` expose `import AVIDCore` to Swift. Link the static
archive plus Foundation, AudioToolbox, VideoToolbox, CoreFoundation, CoreMedia,
CoreVideo, Security and SystemConfiguration; macOS FFmpeg also requires
CoreServices. System `m`, `pthread`, `iconv` and the appropriate C++ runtime may
be required by Rust or the software profile. Native FFmpeg/image archives are
included in Core's Rust staticlib. The source prefix's static pkg-config metadata
records the FFmpeg framework/runtime requirements. Use the C ABI wrapper in
`examples/apple-linked/AVIDCore.swift`; media policy stays in Rust.

A small UIKit host creates local Unicode-path fixtures, probes, produces PNG
previews, renders both compositions and modes, receives progress, cancels an
export while preserving the old destination, and decodes the MP4 through
AVAssetReader. Build it screen-free:

```sh
python3 scripts/linked/build_apple_host.py --target ios-simulator-arm64 \
  --core-library /tmp/avid-core-ios-sim/aarch64-apple-ios-sim/release/libavid_core_ffi.a \
  --output /tmp/AVIDLinkedHost.app
xcrun simctl install booted /tmp/AVIDLinkedHost.app
xcrun simctl launch booted org.avidcore.LinkedHost
xcrun simctl get_app_container booted org.avidcore.LinkedHost data
```

Inspect `Documents/result.json` in that returned container; require `status:
passed` and actual decoded frame/audio counts. A failed hardware encoder is a
failed export, not a simulator pass. Device builds use `--target ios-arm64` and
must be signed/provisioned by an authorized host developer before installation.
No credential enrollment or account change is part of this build. Library
artifacts are not notarization targets; the UIKit app is a development/test host.
A deliberate distributable macOS host must separately follow the repository's
Developer ID/notarization workflow, Apple Accepted status, stapling and Gatekeeper
verification.

## Windows/Linux shared packages

For Windows GNU/LLVM builds, pass `--cxx-runtime c++` for an LLVM/libc++
toolchain or `--cxx-runtime stdc++` for the corresponding libstdc++ toolchain to
both FFmpeg and Whisper builders. The runtime choice is recorded in both SDK
identities and must agree in a combined Core build. Windows MSVC's automatic
runtime linkage applies only to an MSVC inference SDK. GNU/gnullvm builds
explicitly link the selected C++ runtime.

For desktop shared packages, pass the target `.dll` or `.so` instead of the static
archive to `package.py`. Repeat `--runtime-dependency /exact/path/library` for
all non-system dynamic dependencies. The packager rejects development-machine
Mach-O/ELF search paths and missing dependency closure; Windows DLL imports are
read with `llvm-readobj --coff-imports`. Include all compiler-runtime notices and
exact installed compiler identities when redistributing those runtimes. Platform
system libraries and a distribution's standard C/C++ runtime are recorded as
external dependencies, not silently copied from this machine.

Windows `vcruntime140.dll` and `vcruntime140_1.dll` are redistributables rather than
assumed operating-system libraries: if imported, supply them and their transitive
runtime dependencies explicitly, with the applicable redistribution notices.

C/C++ hosts include `avid_core.h` and link Core's library. Use the same request
schema and cancellation/progress rules as Swift/JNI. A C++/Qt host supplies UTF-8
paths and performs no media subprocess work. Run `examples/c-linked/main.c`
against each native package and verify output decoding before claiming native
support. No Windows or Linux native result is implied by the Apple builds.

ELF validation reads the actual `GLIBC_*` symbol-version requirements in the
linked shared artifact. A binary requiring a version newer than the requested
`glibc 2.28` baseline is rejected for packaging; build with the appropriate target
sysroot instead. A native Ubuntu 24.04 compile/smoke result does not establish a
2.28 release baseline. Static archives must also be assessed in the final host
link, whose runtime contracts can differ from the companion Core shared build.

## Android and 16 KB pages

Build the two Android targets with `--profile gpl-software --ndk /exact/ndk/path`.
The NDK revision is checked before a compiler runs. FFmpeg/x265 and Whisper
use one **shared** `libc++_shared.so`; an inference build explicitly selects
`ANDROID_STL=c++_shared`. The exact NDK runtime, SHA-256, revision and original
notices are copied into each SDK's `runtime/` inventory. Both Rust bridges verify
those bytes, and a combined build rejects different runtime identities. The
packager automatically includes this SDK-owned redistributable and notices in
the candidate. It is never treated as an Android system library, and the
inference bridge does not also link a static libc++ copy. Pass `--ndk` and the
same NDK's official CMake toolchain to `script/build_whisper.py`. Export Cargo's target linker,
CC/AR and CMake toolchain paths explicitly; for ARM64 on macOS, for example:

```sh
export AVID_NDK=/path/to/android-ndk-r28c
export CC_aarch64_linux_android="$AVID_NDK/toolchains/llvm/prebuilt/darwin-x86_64/bin/aarch64-linux-android26-clang"
export CXX_aarch64_linux_android="$AVID_NDK/toolchains/llvm/prebuilt/darwin-x86_64/bin/aarch64-linux-android26-clang++"
export AR_aarch64_linux_android="$AVID_NDK/toolchains/llvm/prebuilt/darwin-x86_64/bin/llvm-ar"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CC_aarch64_linux_android"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS='-C link-arg=-Wl,-z,max-page-size=16384 -C link-arg=-Wl,-z,common-page-size=16384'
export CMAKE_TOOLCHAIN_FILE_aarch64_linux_android="$AVID_NDK/build/cmake/android.toolchain.cmake"
```

Use the x86_64 equivalents for the emulator target. Package `libavid_core_ffi.so`
and every required non-system shared dependency. The pinned shared NDK C++
runtime is mandatory and bundled automatically from the verified SDK inventory. Each ELF load segment must have alignment at least 16 KB
and matching file/virtual-address congruence; the packager also rejects an
unaligned RELRO segment sharing its trailing page with writable data. The JNI
library and final APK must be checked too. Use AGP 8.5.1+ with suitable uncompressed
library packaging, verify ZIP alignment with `zipalign -c -P 16`, and execute on a
16 KB emulator/device. ELF alignment alone is not a native runtime pass. These
checks follow [Android's native page-size guidance](https://developer.android.com/guide/practices/page-sizes).

Mobile hosts must obtain platform permissions and import media into accessible
local storage. Security-scoped Apple URLs require host access management;
Android `content://` URIs require host import. Neither URI is an ordinary Core
filesystem path. No unsupported I/O adapter is implied.

## Weekly smoke compatibility

The existing Saturday-only four-target Linux/Windows smoke job detects whether
the exact source tag uses linked Core. Linked tags build the pinned FFmpeg SDK
as a local build prerequisite, compile the native Rust library and run the
compatibility suite plus one small linked probe/preview/export smoke. Native
Windows uses a matching GNU/LLVM target and compiler rather than mixing MSVC
Rust with MinGW libraries. Older source tags retain their original build path.
There is no macOS job, new trigger, retry, full qualification suite, artifact
publication or approval/status gate. No workflow was dispatched for this task;
Windows/Linux execution of the updated path remains unrun until an eligible
future tag is processed. Local workflow/controller tests and actionlint are
reported separately from native execution.

## Licensing boundary

The profile labels describe the exact FFmpeg combination. The Apple profile
omits GPL/nonfree FFmpeg parts; the software profile enables x264/x265 and their
GPL terms. This matches [FFmpeg's license guidance](https://ffmpeg.org/legal.html)
and the original notices retained with the pinned source.

**AVID Core itself remains GPL-3.0-or-later.** An LGPL FFmpeg build does not grant
permissive rights to Core, relicense it, or establish App Store clearance.
Distributors must retain original copyright/license notices and corresponding
source/build information. A static LGPL combination needs the applicable
relinking or corresponding-source route and must respect modification/reverse
engineering rights in the [LGPL 2.1 text](https://www.gnu.org/licenses/old-licenses/lgpl-2.1.html).
The GPL Core source distribution route and the LGPL library's obligations must
both be assessed for the exact combined host package. Separate permissions for
Core-owned copyright, third-party rights, store terms and any codec patent
obligations remain distribution decisions; no permission or clearance is claimed
by this migration. SDK/framework and model licenses remain separate inventories.

## Rust runtime notices

A Rust staticlib/cdylib embeds target sysroot code in addition to Cargo-managed
dependencies. The build receipt records the exact compiler identity, installed
target runtime archive hashes, component manifest and bundled copyright/license
notice hashes. Packaging checks that identity again and retains the official
Rust COPYRIGHT-library.html, complete COPYRIGHT.html and supplied license texts
under licenses/rust-runtime. This includes MIT/Apache notices, LLVM exceptions
and applicable target-specific terms. Cargo vendoring alone is insufficient for
these sysroot components. The retained official inventory is inclusive and does
not claim every listed component is present in the final binary.
