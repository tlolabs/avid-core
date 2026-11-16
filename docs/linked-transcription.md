# Linked transcription: inventory, host contract, and qualification

This work preserves the directly linked Apple providers and adds a separately
identified, portable CPU Whisper adapter. It does **not** complete portable
Parakeet/Qwen inference or forced alignment. An Apple deployment declaration or
shared C interface is not evidence of native mobile inference.

## Subsystem completion inventory

| Public capability | Implementation and exact dependency boundary | Platform/format | Acceptance evidence and remaining work |
| --- | --- | --- | --- |
| Transcript validation, single/batch corrections | `AVIDTranscription.Transcript`; safe Rust `avid-transcription` | Swift Codable/portable Rust JSON, source-relative seconds | Swift/Rust shared-fixture provenance round trips and contract tests; edits clear only affected words and invalidate transcript-wide alignment provenance |
| Captions, trims/timeline mapping, SRT/WebVTT | Existing Swift `CaptionBuilder`/`CaptionExporter`; portable Rust contract | No inference SDK/helper | Tests cover Unicode, source offsets, trims, missing timing, subtitle injection and submillisecond quantization; animated burn-in remains outside this feature |
| Whisper recognition | Preserved `WhisperKitProvider`, Argmax revision `9f99b5cb6a907730543823ad271b82532782a899` | Core ML model artifacts and pinned local tokenizer | Apple adapter contract tests pass, including SDK pin consistency; older real large-v3 results remain historical; no new large-v3 weights downloaded or inference run |
| Parakeet recognition | Preserved `FluidAudioProvider`, revision `ee78635d23d52379117554ba72358b65a0612447` | Core ML; Apple runtime | Full adapter suite builds/runs on macOS ARM64; native provider inference result is recorded separately below |
| Qwen recognition/alignment | Preserved `QwenASRProvider`/`QwenAlignmentProvider`, MLX Audio revision `df2a2546705f50ca267037702a34cd63aa6e35a6`, MLX 0.33.0 | MLX safetensors, Apple silicon | Real offline recognition, corrected-text alignment, loading cancellation and repeated calls pass on macOS ARM64. Recognition remains untimed until alignment. No nonApple Qwen adapter |
| Portable Whisper recognition | `avid-transcription` + narrow `avid-whisper-bridge`, whisper.cpp 1.8.2 commit `4979e04f5dcaccb36057e059bbaed8a2f5288315` | CPU, exact GGML tiny English manifest | Actual offline macOS ARM64 inference, repeated jobs, progress, cancellation and 12.5 s source offset pass; additional native platforms unqualified |
| Portable word timing | Native Whisper BPE token timestamps grouped at actual whitespace boundaries | Exact English tiny model | Actual timing/caption smoke passes. Recognizer timing is experimental, not forced alignment or acoustic ground-truth validation. Missing native times remain missing; no interpolation |
| Portable forced alignment | Explicit unsupported capability | No supported nonApple aligner/model supplied | `transcription_align` returns `unsupported_capability`; never recognizes corrected text again. This requested scope remains incomplete |
| Model install/download/import/verify/remove | Existing Swift `ModelStore` with exact catalog, hashes, resumable HTTPS transport and cross-process leases; portable Rust immutable-manifest import/verify/remove | Separate CoreML/MLX/GGML installation identities | Tests cover real HTTP fixtures, first-attempt aliased-cache installs, symlink rejection, cancellation, copied-file verification and removal exclusion. Rust leases expose verified manifests read-only. Rust download/resume is not implemented; hosts must explicitly obtain exact GGML bytes then import |
| Linked audio decoding/resampling | Existing Apple AVFoundation `PCMChunkReader`; portable libavcodec/libavformat/libswresample `decode_audio` | Local files, 16 kHz mono windows | Apple stereo/gap tests; Rust PCM/WMA/ALAC/Vorbis/Opus/AAC decoding, sample counts, priming, real delay, negative/discontinuous clocks, cancellation/panic cleanup and URL rejection. Additional codec/container timestamp corpora remain needed |
| Resource lifecycle | Swift awaited cleanup and scoped model leases; Rust synchronous native ownership and persistent model locks | Per-operation contexts; independent model instances | Synchronous ownership; PCM bounded to 30 seconds plus one decoder frame. Native same-process media export plus Whisper recognition, independent cancellation and repeated Qwen alignment pass. Cross-model long-duration memory, OS interruptions and device thermal qualification remain outstanding |

The Rust transcript JSON uses the same field spelling as Swift, including
`engineID`, `modelID`, `startSeconds`, `alignmentProvenance`. Call `validate()`
after deserialization of plain mutable Rust records. `Transcript::from_json`
validates automatically. Correcting a segment never keeps its old word timing.
Wordless corrected text cannot export timed captions until explicitly aligned.

New recognition/alignment provenance can additionally include:

- `modelIdentity`: schema version 1, unchanged installation ID, primary repository
  and revision, full `manifestSHA256`, and every installed artifact's path, byte
  count, SHA-256, source repository/revision and source path. Independently pinned
  tokenizer files retain their own source identities. A GGML file's full hash also
  covers its embedded tokenizer.
- `runtimeRevisions`: exact Git revisions for known SDK components. Apple provider
  pins are checked against `Package.resolved`; portable Whisper records its pinned
  upstream source revision.
- `runtimeBuildSHA256`: the digest of the actual validated native SDK build
  receipt, where exposed. That receipt identifies the linked libraries and any
  AVID safety patches separately from the upstream base revision. Preserve it
  with the host artifact; Apple SDK packages currently expose source pins rather
  than a native receipt digest.

The full manifest digest identifies the original model descriptor/manifest, not
the serialized `modelIdentity` object. Existing Swift cache IDs keep their short
digest suffix; the complete digest and per-artifact pins are now persisted too.
Model installation IDs and upstream model/SDK license fields do not change merely
because the native runtime receives an administrative or exception-safety patch.
The modified runtime's combined rights and patch hashes belong to its separate
SDK inventory.

The additive fields remain optional in JSON schema 1. Old transcripts decode
with unknown identities; Core never looks up today's catalog pins and attributes
them to historical recognition. Swift's existing provenance/request initializer
labels remain supported with omitted identities. New hosts should construct
`RecognitionRequest(audioURL:installedModel:...)` or
`AlignmentRequest(audioURL:installedModel:transcript:...)` inside
`withInstalledModel`, so the provider receives the verified descriptor's identity.
Direct older requests retain an unknown model identity while reporting the SDK
actually executing the new operation.

Rust hosts use `Provenance::new` for unpinned/legacy records and
`lease.manifest()` to read the immutable verified manifest. This newly introduced
0.1 crate no longer permits changing the manifest label after acquiring a lease.
Offsets retain both provenance objects; text correction preserves recognition
identity and clears the affected timing/alignment claim. Caption layout does not
mutate the source transcript. Keep that transcript as a sidecar: SRT/WebVTT cue
formats do not carry the complete provenance schema.

The Swift cache-path fix preserves the normalized spelling captured at the
cache/import boundary. Foundation can abbreviate an existing `/private/tmp`
prefix to `/tmp` while leaving a missing child's spelling unchanged; reapplying
that transformation during containment checks incorrectly rejected a fresh
cache's lock directory. Containment now compares stable paths and still checks
every descendant for symlinks. The regression installs, verifies and removes a
fixture on its first attempt under both aliases, without creating a cache during
initialization. Existing symlink-escape tests remain passing.

The portable decoder supplies the stream's packet time base to libavcodec.
FFmpeg then consumes codec skip/discard metadata and adjusts the timestamps of
partially trimmed AAC/Opus frames itself; Core does not manually discard them
again or clamp arbitrary negative timestamps. A WMA decoder's final drained
frame can lack a timestamp: once an earlier frame anchors the source clock,
the exact decoded sample count determines that continuation's start. A missing
initial timestamp still fails, and every available timestamp remains subject to
discontinuity validation. Genuine track delay is emitted as silence.

## Portable runtime and model identity

The portable profile has no default model substitution. Hosts explicitly choose
`whisper-cpp-tiny-en`; it is an additional English-only model, **not** a replacement
for the Apple default full Whisper large-v3. CoreML/MLX files do not load in this
runtime. Unsupported model manifests/languages fail before inference.

| Input | Pin |
| --- | --- |
| whisper.cpp source | [commit 4979e04](https://github.com/ggml-org/whisper.cpp/tree/4979e04f5dcaccb36057e059bbaed8a2f5288315) |
| Source archive SHA256 | `ec8e33f4c838ed459ad0a23da255c0262f150a6f54839a17e504ddf1495305d5` |
| GGML publisher/revision | `ggerganov/whisper.cpp` at `5359861c739e955e79d9a303bcbc70fb988958b1` |
| Exact model | `ggml-tiny.en.bin`, 77,704,715 bytes |
| Model SHA256 | `921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f` |
| Upstream base SDK license | MIT, [preserved notice](../licenses/linked-inference/whisper.cpp-MIT.txt); GGML included in the pinned source. The modified SDK has the combined rights described below |
| Model license | OpenAI Whisper MIT; preserve publisher and original model notices separately from SDK notices |
| Core and adapter license | GPL-3.0-or-later; adding an MIT SDK/model does not relicense Core |

`script/build_whisper.py` consumes the exact source archive without network
access, uses a fresh build tree, disables executables, native-host instruction
tuning, GPU and implicit model acquisition, and records target/compiler/SDK/CMake
configuration, exact source identity and static-library hashes in
`avid-whisper-build.json`. The Rust build rejects a missing receipt, wrong target,
wrong revision/version, escaped library path or changed archive hash. This is a
local build provenance receipt, not a cryptographically authenticated publisher
manifest. It is separate from all historical FFmpeg executable contracts.

### Native exception boundary

The portable SDK variant is `avid-exception-safety-r1`: the original archive stays
unchanged and a [separately hashed patch](../licenses/linked-inference/patches/manifest.json)
changes partial context/state/backend/KV ownership, CPU allocation replacement and
worker exception cleanup. AVID's patch is GPL-3.0-or-later; upstream MIT notices
remain intact, so the modified SDK is `MIT AND GPL-3.0-or-later`. SDK and artifact
packages retain the patch, its manifest and both full license texts beside the
unchanged base source. This does not change model identities or model rights.
The Rust build and package tooling reject older unpatched SDKs, altered patch
identities, missing notices and changed native archives. Transcript
`runtimeBuildSHA256` identifies the exact validated SDK receipt, including this
variant and its archive hashes.

The narrow C++ bridge exposes `extern "C" noexcept` functions. It scopes model
FILE/context ownership and converts `std::bad_alloc`, other `std::exception`
and unknown exceptions into bounded native error records with operation, code,
kind and detail. Native inference status failures or C++ exceptions poison the context; callers unload it rather
than retrying on partially updated state. Worker exceptions are transferred only
after every started worker joins. Short audio is zero-padded to the native
reflect-padding minimum before inference. No C++ exception crosses into Rust.

Containment covers the exposed CPU, non-DTW, ordinary Whisper inference path.
It does not promise recovery from upstream C allocation failures, assertions,
explicit aborts, process termination, arbitrary throwing host callbacks, or
unsupported experimental/AMX configurations. Native instruction tuning, GPU,
OpenMP and experimental DTW are disabled in this profile. Out-of-memory recovery
is therefore not a universal guarantee.

The opt-in native harness overrides allocation in the actual compiled SDK and
injects failures during model construction and inference/worker setup. It
requires either a bounded structured failure or successful real inference,
checks tracked C++ allocation/FILE cleanup, and verifies failed contexts cannot
be reused. It exercises selected allocation positions rather than every upstream
failure site. Supply the exact already installed tiny English model explicitly:

```sh
python3 scripts/linked/test_whisper_native.py --prefix /absolute/whisper-sdk \
  --model /absolute/ggml-tiny.en.bin --output /absolute/outside-repo/native-fault-test
```

This is native host evidence on macOS ARM64; cross compilation alone does not
qualify exception recovery or inference on the other platforms.

```sh
python3 script/build_whisper.py --source-archive /absolute/whisper-pinned.tar.gz \
  --build /absolute/fresh-build --prefix /absolute/whisper-sdk \
  --target aarch64-apple-darwin
# Supply an already built, pinned linked FFmpeg SDK as documented separately.
AVID_WHISPER_PREFIX=/absolute/whisper-sdk \
AVID_FFMPEG_PREFIX=/absolute/ffmpeg-sdk AVID_FFMPEG_STATIC=1 \
  cargo build --locked -p avid-core-ffi --features whisper
```

Build-script target choices keep macOS ARM64/Intel, iOS device ARM64,
iOS simulator ARM64/Intel, Windows/Linux x86_64/ARM64 and Android ARM64/x86_64
distinct. Cross builds require the correct target compiler/toolchain. Android
uses minimum API 28 and flexible 16 KB page flags, but final shared libraries still
require ELF load-segment verification and real device inference. Static archives
alone cannot demonstrate 16 KB compatibility. iOS support requires separately
packaging the inference archives with the Core/FFmpeg target slice. Those combined
mobile inference artifacts have not been qualified by this work.

## Direct host API and ATIV handoff

Apple hosts can replace the transcription helper process immediately with the
existing Swift libraries. Add both local package dependencies from the same pinned
Core checkout, select the exact catalog model, obtain deliberate installation
consent, then hold `ModelStore.withInstalledModel` through the complete provider
call. Providers unload before returning. Forward `Sendable` progress values to the
UI, cancel the owning Swift task, and wait for it to finish before releasing
resources. CoreML/Metal kernels can delay cooperative cancellation. The complete
example in [apple/README.md](../apple/README.md#host-integration) remains applicable.
Use `correctingSegments` for batch edits, `QwenAlignmentProvider.align` for supported
explicit forced alignment, then `CaptionBuilder`/`CaptionExporter` for captions.

Windows/Linux/Android C++/JNI hosts use the versioned Core JSON ABI. Build with
`--features whisper` and the pinned target inference SDK to enable recognition.
No process, Python interpreter, helper or service is invoked by these operations.
`transcription_capabilities` explicitly reports whether inference was compiled.
Without that feature only model-management and transcript/caption contracts are
available. Swift providers are separately linked Swift libraries and are not
pretended to exist behind this C ABI.

Every request has `schema_version:1` and one operation:

- `transcription_models`: exact supported manifests; no implicitly selected model.
- `transcription_model_import`: `cache`, explicit `model` manifest, `source` file.
- `transcription_model_verify`/`transcription_model_remove`: `cache`, explicit model.
- `transcription_recognize`: `cache`, explicit model, `audio`, optional `language`
  (`en` only for this model), `source_offset`, `threads` 1–64. The operation verifies
  all bytes under a model lease, loads, streams inference and unloads before return.
- `transcription_validate`: `transcript`; returns validity and timing completeness.
- `transcription_correct`: `transcript`, `corrections:[{"index":0,"text":"edit"}]`;
  all indices must be valid and unique; edits apply atomically.
- `transcription_srt`/`transcription_webvtt`: `transcript`, optional grouping
  `options` and source/timeline `window`; returns text/cues without writing a file.
- `transcription_align`: explicit unsupported result. NonApple forced alignment
  still requires an implemented, pinned adapter/model before ATIV can migrate it.

Foreign ownership, borrowed callback spans and synchronous worker lifetime follow
[the Core ABI contract](linked-api.md). Progress uses the same caller-thread
event callback as media. Cancellation may come from another thread; the caller
must await the executing call. Do not recursively use the same native model
context or mutate installed files. Cache roots must be private host-controlled
directories; persistent model-lock inodes must not be replaced externally.

Hosts import media into accessible local storage first. Apple security-scoped
URLs require host-managed permission/access lifetime. Android `content://` URIs
are imported through the document resolver; neither URI nor a security permission
token is treated as a filesystem pathname. Supply a platform sandbox/cache path
explicitly to the portable store. There is no cloud fallback or automatic runtime
model/tokenizer download. ATIV/EnCAP source and pins were not modified.

## Verification recorded November 16, 2026

On macOS ARM64, Swift 6.4.2 / Xcode 27.2:

- Swift core: 40 XCTest tests pass after the cache-path and provenance fixes, including first-attempt
  installs under `/private/tmp` and `/tmp`, atomic batch correction, transcript
  validation, HTTP resume, integrity, symlink and model-lifecycle coverage.
- Swift engine: 16 native adapter contract tests pass after a fresh full pinned
  dependency/Metal build. This includes real AVFoundation PCM decoding and delayed
  audio-track silence, while inference model execution is distinct evidence.
  A restricted rerun failed the two real AVFoundation audio fixtures with
  `AVFoundationErrorDomain -11800` / underlying `NSOSStatusErrorDomain -1`;
  the native-permission rerun passed all then-current 14; the later provenance
  suite passes 16. Both outcomes are retained in the
  task's verification logs.
- Actual preserved Apple Parakeet v3 inference also ran offline against the existing
  SHA256-verified Core ML installation on the same synthetic speech. Loading,
  recognition progress and awaited `AsrManager.cleanup` completed successfully.
  This remains macOS ARM64 evidence, not iOS/device qualification.
- Swift core: ARM64 iOS 17 simulator library cross-build passes; no simulator runtime
  inference or physical device result is implied.
- Portable contracts: 9 Rust tests pass; linked audio: 5 tests pass. The expanded
  audio regressions include partial AAC priming, WMA's timestamp-less drained
  frame, Opus skip metadata, real initial delay and negative/discontinuous clocks.
- Portable real inference: 3 explicit ignored-by-default tests were actually run and
  pass with the patched native SDK, exact tiny English weights, synthetic 5.5-second speech, repeated contexts,
  real native word timing/caption export, progress, 12.5-second offset, pre-cancellation,
  inference-completion cancellation and subsequent model removal.
  The third real-model regression verifies that corrupt audio retains the
  decoder's native error code/operation/detail rather than becoming a generic
  input error. The same actual native C ABI error also retained its libav code
  and operation in JSON; the failure was expected by that regression.

The shared synthetic fixture `tests/fixtures/transcript-provenance-v1.json` is
decoded and round-tripped by both Swift and Rust. Tests preserve all optional
identity fields, including a separately pinned tokenizer and native build digest;
reject malformed revisions/hashes, unsafe or colliding paths and unsupported
identity schemas; and retain explicit unknown pins in legacy JSON. Real offline
Parakeet recognition was repeated with the new request API: its 17-word transcript
contains all 21 verified artifact identities, the unchanged installation ID, the
full descriptor digest and the pinned FluidAudio revision. The fixture's hashes
are synthetic test data, not claims about downloaded models.

The patched-SDK C ABI host was then run with no media executables in `PATH`.
Its real Tiny transcript's `runtimeBuildSHA256` matched the validated SDK receipt
(`26c1f18467bf9de9390438aab6c4565641ba9dd4aed732e3aeb437729912c10b`), and its
model identity retained the actual GGML file hash and unchanged installation ID.
Swift decoded and validated that complete JSON, including the new immutable
provenance, and exported WebVTT successfully. This cross-language smoke covers
the recorded macOS ARM64 artifact, not other target qualifications.

Actual preserved Apple Qwen models were deliberately installed into a temporary
task cache. The catalog verified their exact sizes and SHA-256 digests before
publication; inference then ran with `--offline` and no media executables in
`PATH`. SDK and model licenses remain separate:

| Model | Immutable model revision | Native result |
| --- | --- | --- |
| Qwen3-ASR 1.7B 4-bit, Apache-2.0 | `78a389c776a5483b2d0d4ea5494e11012e0d6159` | Recognized the complete 17-word English source; `mlx` recognition provenance retained. Word times remain absent; caption export correctly rejected the untimed result and created no subtitle file |
| Qwen3 ForcedAligner 0.6B 4-bit, Apache-2.0 | `2f652af86ae0c73fe189b9429225c908ce4bf020` | Aligned 17 words on the source clock, approximately 0–5.24 seconds, while preserving the exact corrected text and punctuation. The original portable Whisper recognition provenance and the Qwen alignment provenance remain separate |

For the alignment check, the C ABI's batch correction operation first replaced
two recognized segments and cleared all affected word timing. The Swift aligner
then consumed that corrected transcript without another recognition pass. A
temporary direct Swift library host also cancelled after observing Qwen's loading
stage, awaited teardown and lease release, and completed two further alignments
through the same provider actor in one process. All three private model/tokenizer
staging directories were gone after return. Cached loading and real alignment
were rerun successfully against the Swift cache-path fix. These are lifecycle
and pipeline checks, not acoustic ground-truth timing or general accuracy tests.

The fixture was synthesized locally with macOS speech synthesis (fixture creation
only); production inference uses no speech helper. Its SHA256 is
`aba2d8c896a7c480dbcff3aa35d1a4bc9d946b3ea7eff64fb58da0628a84d88e`.
Tiny English recognized 16 words; one article differs from the 17-word source. This
is a pipeline smoke, not an accuracy or acoustic timing benchmark.

To reproduce real portable inference, provision the exact weights and an English
speech fixture explicitly, then run:

```sh
AVID_WHISPER_PREFIX=/absolute/whisper-sdk \
AVID_FFMPEG_PREFIX=/absolute/ffmpeg-sdk AVID_FFMPEG_STATIC=1 \
AVID_TEST_WHISPER_MODEL=/absolute/ggml-tiny.en.bin \
AVID_TEST_SPEECH=/absolute/speech.aiff \
  cargo test --locked -p avid-transcription --features whisper --test inference \
    -- --ignored --test-threads=1
```

Native process memory was measured with `/usr/bin/time -l`. The portable rows
below use the C host linked to the pinned static GPL FFmpeg profile and pinned
Whisper CPU runtime, with media executables absent from `PATH`; they supersede
earlier Homebrew development-SDK measurements. Source/model loading and
transcript-output allocations are included. These runs preceded later native
bridge audit fixes; final artifact identities and affected reruns are recorded
with the overall task's verification evidence.

| Native operation on macOS ARM64 | Maximum resident set size | Darwin peak memory footprint | Wall time |
| --- | --- | --- | --- |
| Portable tiny English, 5.453-second speech | 162,807,808 bytes (155.3 MiB) | 154,862,168 bytes | 0.34 s |
| Portable tiny English, 654.411-second repeated speech | 194,002,944 bytes (185.0 MiB) | 186,008,176 bytes | 9.79 s |
| Apple Qwen ASR, same short speech | 3,387,244,544 bytes (3.15 GiB) | 4,793,257,872 bytes | 2.42 s |
| Apple Qwen forced alignment, two corrected segments | 2,120,728,576 bytes (1.98 GiB) | 3,430,665,048 bytes | 6.25 s |
| Apple Qwen loading cancellation followed by two alignments in one process | 2,129,707,008 bytes (1.98 GiB) | 2,638,023,632 bytes | 8.00 s |

The long portable recording produced 295 segments and 1,916 recognized words.
An approximately 120-fold input-duration increase raised peak RSS by about
30 MiB; this supports bounded PCM/model working memory for this profile and
fixture. The transcript itself grows with recognized text. Qwen's download size
is substantially smaller than its measured runtime footprint. Darwin RSS and
memory footprint are different metrics; neither establishes another device's
memory budget or the absence of every possible leak.

A temporary pthread C host additionally exercised a shared Core handle with
independent cancellation tokens and synchronous callback contexts in one process:

| Concurrent case | Result | Peak RSS |
| --- | --- | --- |
| 120-second media export plus actual Whisper recognition | Both succeeded, delivered progress, and the export emitted one Complete stage | 179,585,024 bytes |
| Cancel export at its first progress callback | Export returned `cancelled`, emitted no Complete stage, and preserved the existing MP4's SHA-256; recognition still succeeded | 168,443,904 bytes |
| Cancel recognition at its initial loading progress callback | Recognition returned `cancelled`; export still succeeded and emitted Complete | 26,771,456 bytes |

The callback also reentered an independent buffer query without deadlocking the
handle registry. Independent verification decoded the resulting MP4 completely
and confirmed 120.000 seconds of H.264 at 160×90 with AAC at 44.1 kHz mono.
The FFmpeg/ffprobe executables used for that separate verification were not
available to the production host's `PATH`. Two independent C host processes
also recognized the same installed model concurrently under shared Rust leases.

Qualification remains specific to a platform, provider, model and artifact:

| Platform/provider combination | Compilation or packaging evidence | Native execution evidence |
| --- | --- | --- |
| macOS ARM64, portable tiny English CPU | Pinned FFmpeg/Whisper/Core artifacts built and C host linked | Recognition, progress, cancellation, repeat/concurrent jobs, long-input memory and export contention passed as described above |
| macOS ARM64, Apple Parakeet/Qwen | Pinned Swift packages and Metal dependencies built | Parakeet recognition, Qwen recognition/forced alignment and Qwen lifecycle checks passed |
| macOS ARM64, full WhisperKit large-v3 | Adapter contract build/tests passed | Previous November 2 inference remains historical; no new full large-v3 inference run |
| macOS Intel, portable tiny English | Whisper CPU archive cross-build passed; Core packaging tracked separately | No Intel native inference run |
| iOS ARM64 device, ARM64/Intel simulator, portable tiny English | Distinct Whisper archives cross-built; object metadata verified iOS versus iOS Simulator, minimum iOS 17; combined Core packaging tracked separately | No simulator or physical-device inference run |
| iOS Swift contract/provider packages | Contract library cross-built for ARM64 simulator; provider deployment declarations exist | No provider/model/device inference qualification |
| Windows/Linux x86_64/ARM64 and Android ARM64/x86_64, portable tiny English | Explicit target build recipes supplied; consult artifact records for actual build coverage | No native inference run on these targets |
| Any nonApple platform, Qwen/Parakeet/forced alignment | Portable adapters remain unimplemented | Unsupported; no silent model or timing substitution |

No native Windows/Linux/iOS/Android inference result, Intel Mac inference, mobile
physical-device result, portable Qwen/Parakeet replacement, cross-model long-recording
memory qualification, forced-alignment acoustic accuracy or cross-model quality
equivalence is claimed. OS interruption/thermal testing and sustained Qwen memory
or concurrent same-model Apple inference qualification also remain open. Original
Apple SDK/model license records and older verification remain preserved under
`apple/`; runtime acquisition policies and CI schedules are unchanged.
