# Linked migration status and subsystem inventory

Baseline: `8e27a86ec106028d93130de35a35c0eeda7a9ce3`.
Task: `avid-linked-20261116-8e27a86` (2026-11-16).
Original requirements, pre-existing changes, logs, source/review identities and
administrative model override are preserved outside the repository under
`/private/tmp/avid-linked-20261116-8e27a86` and the existing review runner state.
Existing untracked reports/screenshots were not removed or edited.

**The complete requested platform/provider migration is not qualified or complete.**
Media now has one production linked implementation and native macOS ARM64 evidence.
Other target builds/runtime coverage and portable forced alignment/provider parity
must not be inferred from that success. ATIV/EnCAP shipping applications remain
unchanged. The original migration handoff published no pins, tags, stores or
releases. The owner subsequently requested the `v0.4.0` source-library release
on 2026-11-16. That release does not qualify compiled candidates or change
downstream pins; the platform and provider restrictions below still apply.

## Completion inventory

| Public capability | Implementation / dependencies | Executable/platform boundary | Acceptance evidence / remaining work |
| --- | --- | --- | --- |
| Presets, state, settings, ordered selection | Core model/presets/state, serde | Portable Rust | Compatibility tests retain 27 presets and state extensions/defaults |
| Audio duration and all consumed stream metadata | linked libavformat/libavcodec probe | Direct local media, all target code paths | Real source/output probe, duration/SAR/rates/start times; target native qualification outstanding |
| Image inspection/decoding/orientation | libavcodec, safe streaming RGB bridge | Same linked path; local direct formats | PNG/JPEG, EXIF6, GIF/APNG decoded tests, source/output limits |
| JPEG optimization | pinned libjpeg-turbo/TurboJPEG | Already linked; target CMake compiler | 23 optimization + 4 sanitization tests, coefficients/metadata/opacity/alias safeguards |
| PNG optimization | OxiPNG/libdeflate/Zopfli/png | Already linked; target C compiler | Alpha/16-bit/Adam7/animation rejection and limits tests |
| Relevant filter / encoder capabilities | compiled libavfilter / avcodec initialization | Hardware availability distinct from compiled support | Actual representative VideoToolbox initialization, compiled-only software enumeration; full export evidence only where executed |
| PNG previews | canonical artwork graph, PNG encoder | No executable discovery | Both compositions, concurrent/repeated jobs, aliases and failure preservation |
| Single export / timelines / hard cuts | shared typed plan, libavfilter, AAC + H.264/HEVC, MP4 | All production targets use identical linked path | Actual decoded cadence, audio layout, color cuts, initial silence and duration |
| Simple / PerFrame / animation | canonical composition/filter policy + streaming decode | No complete-video buffering | GIF repeat; APNG plays0/1/2 hold behavior; Simple freeze; single and timeline tests |
| Codec selection / fallback | Core encoding policy, compiled profile | Apple VideoToolbox or separately GPL x264/x265 | Explicit choices and usable initialization respected; Apple GPL hardware+software profile and native error/fallback regressions; hardware unavailable in sandbox, usable outside it |
| Cancellation / deadlines / errors | independent token + per-operation control / FFmpeg interrupts | Cooperative codec/driver interruption | Pre-cancel, progress cancel, deadlines, cleanup and preserved destination; no hard timeout guarantee |
| Atomic output / integrity | same-file identity, staged tempfile publication | Host filesystem | Symlink/hardlink/protected paths, nonempty checks, corrupt input and no Complete on failure |
| C/Swift/C++/JNI integration | ABI1/schema1 opaque handles, safe dispatch | Thin host transports | C and macOS Swift export/progress/cancel with empty PATH and independent decode; iOS host compiled; Android host unrun |
| Apple recognition | AVIDTranscriptionEngines WhisperKit/CoreML, FluidAudio/CoreML, Qwen/MLX | Existing linked Apple frameworks | Swift engine contract tests; actual provider/device qualification listed in transcription document |
| Portable recognition | optional pinned whisper.cpp1.8.2 + GGML tiny.en | Direct C runtime, FFmpeg PCM; English only | Actual offline recognition, native token timing, offsets, cancellation/repeated jobs and same-process export contention; other models/languages not claimed |
| Corrected-text forced alignment | existing Apple QwenAlignmentProvider | Apple MLX/model/device restrictions | Actual corrected-text Qwen alignment and cancellation/repeated teardown on macOS ARM64; no portable forced aligner, capability explicitly false |
| Transcript validation / corrections | existing Swift contracts + portable Rust transcript crate | Direct linked APIs | Single/batch correction invalidates word timing; no invented timing or recognition to replace alignment |
| Caption grouping / SRT / WebVTT | Swift and portable Rust contracts | Direct API; local text output | Timing/provenance, offsets/trims/timeline mapping tests; no animated caption burn-in |
| Model install / verification / leases / removal | existing Apple ModelStore + portable immutable GGML store | Host-selected local sandbox/cache | Exact identities/hashes, bounded verification, leases through unload, cancellation and concurrent removal safeguards |
| Model downloads / resumption | existing Apple deliberate HTTP model transport | Network only on explicit installation | Existing transport tests; portable adapter imports verified files and has no downloader/resume implementation |
| Native packaging and licenses | separate linked spec/build receipts, target binary inspection, sources/notices | Deliberate build tools only | Apple FFmpeg/Rust cross-build evidence; 16 KB Android checks in tooling, target native results not claimed |
| Legacy executable APIs | opt-in `legacy-subprocess` only | Reference testing; excluded from production defaults | Existing fake-process/lifecycle/runtime tests retained; no automatic fallback |

## Platform qualification matrix

| Target | Library / host compilation | Native media execution | Inference / alignment execution |
| --- | --- | --- | --- |
| macOS ARM64 | Pinned Apple LGPL and GPL FFmpeg/Rust built; C and Swift hosts link | Linked software suites pass; Apple VideoToolbox suite passes outside sandbox; C/Swift MP4 independently decoded | Portable tiny.en actual inference passed; Swift contracts/engine tests pass; actual Parakeet/Qwen recognition and Qwen alignment pass; full provider/model/device combinations remain incomplete |
| macOS Intel | Pinned FFmpeg and Core cross-builds; binary members checked | Not run on native Intel | Not run |
| iOS/iPadOS ARM64 device | Pinned FFmpeg/Core cross-build | Not run; detected iPhone unavailable | Not run; no physical-device qualification |
| iOS ARM64 simulator | Pinned FFmpeg/Core and UIKit host compiled | Not run; no installed simulator devices/runtimes | Not run |
| iOS x86_64 simulator | Pinned FFmpeg/Core cross-build passed; binary members checked | Not run | Not run |
| Windows x86_64 / ARM64 | Explicit target builds/tooling supplied; not qualified here | Not run; native toolchain/host unavailable | Not run; linked adapter target portability unqualified |
| Linux x86_64 / ARM64 | Explicit target builds/tooling supplied; not qualified here | Not run; native toolchain/host unavailable | Not run; linked adapter target portability unqualified |
| Android ARM64 / x86_64 | NDK/API/16 KB settings + JNI test application supplied; uncompiled here | Not run; SDK/NDK/emulator/device unavailable | Not run; no Android native inference qualification |

## Verification and limits

Rust workspace checks/tests include production linked tests, existing image,
settings, lifecycle, historical runtime and fake-process coverage. Native real
inference tests are opt-in and executed separately with the exact verified model.
Ignored runtime-pair/legacy full diagnostic tests are not counted as passes.
The build-tool tests remain diagnostic. Commands and output logs are in the task
state; [TESTING.md](TESTING.md) gives reproducible commands and acceptance limits.

A 120-second 160x90/24fps Simple export completed through the C ABI with an empty
PATH. Sampled child peak RSS was 26,476,544 bytes versus 21,135,360 for a one-second
export. This small synthetic measurement supports bounded streaming, not a
universal memory bound for all dimensions, codecs, models or concurrent workloads.
Model download size is not inference peak memory. A separate tiny.en CPU measurement
used 158 MiB peak RSS on 5.45 seconds of speech and 187.5 MiB on 654.4 seconds of
repeated speech; no other model/device/contention memory bound is inferred. Production libraries contain no media executable or
implicit dependency downloader.

The first default-sandbox Apple encoder tests failed VideoToolbox initialization
(-12908); the same profile's five media tests passed when hardware access was
available outside that sandbox. Early native sink/channel-layout, packet duration,
APNG boundary-packet, frame-type, delayed-audio and lifecycle defects were fixed
and affected decoded tests rerun. MSRV, lints/docs, exact final-source tests and
external audit are recorded in the final task evidence, rather than treating an
earlier source build as evidence for later changes.

Remaining implementation restrictions: portable forced alignment, non-Apple
Parakeet/Qwen adapters/model formats/languages, portable resumable model download,
cross-model/device long-inference/contention/OS-interruption qualification and all unavailable
native hosts. Existing Apple CoreML/MLX models do not become GGML models. Missing
capabilities remain errors; Core never substitutes another model or cloud service.
Licensing/App Store clearance, downstream integration, application notarization
and publication are separate unresolved decisions. These are library/development
builds; no notarized distributable app was requested or produced.

## Independent review and corrective verification

The first Antigravity run selected `gemini-4-argon-high` explicitly with no
fallback. It returned seven findings with partial coverage (`validation:passed`,
`status:incomplete`), against full snapshot
`4f828813e1793a71a2728dcea6f8d9b57d38d9ebd73e65acc3395c64fb735a7f`.
All findings and model-attribution limits are retained in the external task state.
A partial report is not a clean audit or native platform qualification.

Corrections cover dependency link ordering, exact Windows C++ ABI selection, one
verified bundled Android shared libc++ runtime, usable-encoder selection and an
Apple GPL hybrid profile, fair sink draining, integer/final audio frames, and AAC
sample-rate negotiation. Supported source rates stay exact; unsupported AAC rates
are resampled to the nearest supported rate. Software jobs avoid GPU initialization.
Native timestamp arithmetic is checked. The precise inevitable failures described
for the timeline-stall and zero-sample findings did not reproduce; their underlying
scheduling/endpoint concerns were corrected and those distinctions are recorded.

Later codec regressions cover BMP/TIFF, literal-percent image paths and the
existing WMA/ALAC/Vorbis/Opus formats. The portable audio decoder supplies the
packet time base before opening the codec, preserving codec priming adjustments;
an un-timestamped WMA flush frame continues only from an established sample clock.
Negative timestamps and actual discontinuities still fail. The source-only smoke
test generates its own PNG/WAV inputs so it does not rely on excluded binary
fixtures. Native inference errors preserve their library, code and kind through
the C ABI. New transcripts carry verified model hashes and runtime pins without
inventing identities for old transcripts.

The portable Whisper boundary is C++ `noexcept` and uses an explicitly pinned
AVID exception-safety patch over the unchanged upstream archive. Native tests
inject real C++ allocation failures in factories and inference workers, check
cleanup, and allow successful upstream allocation recovery. This does not make
upstream fatal C allocation/assertion failures recoverable in process. The patch
and exact combined runtime rights are retained with corresponding source.

The fresh Swift cache alias regression was reproduced independently, fixed without
resolving away unsafe target symlinks, and verified with the core test suite. Native Swift
adapter tests passed with required platform access; a restricted attempt's two
AVFoundation failures remain visible. A second Gemini 4 Argon High audit was
explicitly requested after these fixes; its actual final snapshot/report is kept
in the task evidence and final response. Earlier artifacts correspond to their
recorded earlier source hashes; only rebuilt receipts may describe later changes.

The second report confirmed all seven initial findings resolved and reported no
new findings, but remained partial. Its validation failed because the code-only
snapshot omitted binary smoke fixtures; that test has since been made self
contained. The third focused Argon audit covered the later native exception
boundary, audio and provenance corrections, passed validation and reported no
new findings, but also remained partial. All run results, source hashes, failed
checks and attribution limits remain in the external task state; a partial report
does not qualify unavailable target runtimes.

### 0.4.0 source release continuation (2026-11-16)

The owner's current policy selected Antigravity profile `gemini`, CLI model
`gemini-3.8-flash-high`, with fallback disabled, for one fresh release audit.
The same baseline, task ID, original requirements and state directory were
retained. This fourth historical completed report consumes four of the six
available reports. It passed all seven runner validation commands, confirmed
the seven earlier findings resolved and returned no new findings, but explicitly
reported **partial coverage / incomplete status** (exit 20). There is no accepted
complete independent audit. No independent provider serving-model identity was
exposed; model attribution is the recorded CLI selection.

The reviewed full working-source snapshot was
`76e2958104c4dddb3d1daf1536ef652603b2752b5c73b51da9c1121682703136`;
the actual code-only payload hash was
`86a0d6452981f0919684ea261486864bf60bcd97e45037aded9418c9a0eb3fd5`.
The runner manifest contains 347 text files (the review narrative incorrectly
says 352); binary fixtures and `include/module.modulemap` were excluded. Codex
verified those files through the complete-checkout tests and release archive
integrity checks. The review summary's broad platform-success wording is not
native qualification; the matrix and restrictions above remain authoritative.

Fresh macOS ARM64 release checks passed: an optimized Core/FFI build with the
verified software FFmpeg and patched Whisper SDKs, 158 Rust workspace tests,
three separately enabled real Whisper inference tests, 166 Python tooling
tests, formatting, Clippy with warnings denied, dependency inventory and the
doctest command (zero doctests). The default workspace suite skipped 13 opt-in
tests; after separately running the three inference tests, ten legacy
executable-runtime tests remain unrun. Native fault tests covered 11 factory
allocation faults (ten contained failures and one recovered success) and ten
inference/worker faults. Later edits record release/audit documentation only;
Codex verifies the final committed source archive before publication.

### Media continuation after 0.4.0 (2026-11-16)

The resubmitted request continues the same task/baseline from starting HEAD
`dd112d497b68df6786f49a71b3611f7f32f090e9`. Its exact text and initial working-tree
inventory are retained in the original external task directory. Earlier release
authorization is historical: this continuation authorizes no push, tag movement,
publication, downstream pin or application change. The
[concrete platform handoff](linked-handoff.md) covers ATIV/EnCAP's remaining work.

Four reproduced defects were corrected: publication now rechecks cancellation
and protected paths after flushing and before retrying replacement; audio decode
observes cancellation from its final resampler progress callback; Simple artwork
uses the requested frame clock instead of the demuxer's 25 fps default; JNI
callback exceptions cancel native work and propagate only after cleanup. The
clock defect added 20 ms before the second timeline clip's audio at 24 fps.
Default and explicit reference regressions failed before correction and pass
afterward without shifting decoded audio or widening the comparison tolerance.

Packaging now checks actual Apple binary deployment minima against the combined
media/inference requirement, exact desktop compiler OS/ABI, Windows runtime DLL
closure, source symlinks and source additions during packaging. Existing artifacts
remain associated with their original source identities; inspecting their binary
minimum OS does not turn them into builds of these corrections. New build and
package receipts must describe the source actually used.

Fresh checks passed 165 Rust workspace tests, seven separate historical real-media
tests, the new linked/reference decoded comparison, three optional real inference
tests, seven Apple VideoToolbox media tests, lints, formatting and Rust1.85 library
checks. The default suite ignored 14 opt-in tests; the eight reference/media and
three inference tests were run separately. Three historical installed-runtime
tests remain unrun. Documentation tests ran with zero doctests. Tooling and native
host results and final source hashes are recorded in the continuation logs and
verification JSON outside the repository, with failures retained.

An x86_64 macOS build passed 101 Core tests **under Rosetta on ARM64**; this is
translated execution, not native Intel hardware qualification. The actual JNI
transport/Java wrapper compiled and exercised export/progress, callback-exception
cancellation, preservation, cleanup and subsequent export on a macOS JDK with an
empty media-tool PATH. This is not Android execution. The Android Activity now
independently decodes video/PCM with MediaCodec and writes `files/result.json`;
its Android build and execution remain unrun.

Outside-sandbox inventory confirmed no iOS simulator runtimes/devices and an
unavailable physical iPhone. Android SDK/NDK/emulator, native Windows/Linux hosts
and native Intel hardware are unavailable. No simulator or physical-device pass
is inferred from cross-builds or Rosetta. Original rights/App Store decisions and
unavailable native target qualification remain unresolved. Library candidates and
development hosts are not notarized distributable macOS applications.

The new corrective source requires one fresh Antigravity audit using only profile
`gemini` / `gemini-3.8-flash-high`, fallback disabled, retaining the original state
and four historical completed reports. Its actual report, coverage, model
attribution, findings assessment and source hash are preserved outside the
repository. Prior partial reports are not a complete audit of these fixes.

That fifth completed report returned no findings and passed all eight validation
commands, but explicitly remained partial/incomplete (exit 20). Requested profile
`gemini` selected CLI model `gemini-3.8-flash-high`; no independent provider model
identity was exposed. The reviewed full snapshot was
`7fc6a57d20560d7f6742a8c25033f61d2999d198cb02a26fd207b46073c1843b`,
with code-only payload
`d58d3259cb14f8c0bc68672dfb9d18e73532aaf2eaf2cbd2ee0e32215027925e`.
Its broad narrative does not supersede its partial coverage or qualify native
targets. Five of the six reports in the same preserved history are consumed;
there is no accepted complete independent audit.

Codex's second pass found an additional source-retention defect after that
snapshot: Simple timeline inputs kept decoded full-resolution artwork until the
whole job ended. With 2048x2048 artwork and a 160x90 output, measured peak RSS rose
from about 51 MB for one clip to 365 MB for 16 clips. The later correction bounds each
Simple input to one source frame, letting the existing EOF/decoder cleanup run
while the canonical graph loops the composed output. Source-sized FFmpeg scaling
contexts also retained live intermediate buffers per clip; Simple now prepares
unique artwork sequentially through the existing composition graph into private
PNG files, then feeds output-sized images to the shared freeze/concat policy.
The strict memory regression and decoded reference comparison pass. PerFrame
timing and clip audio bounds retain their existing policy. Tests, memory evidence,
rebuilt receipts and final source hash are recorded separately; the earlier review
did not audit this correction. Neither a small synthetic streaming test nor the
partial external report establishes a universal memory bound.

An additional tiny-clip regression found FFmpeg's positive trim duration rounds
to zero below half an output frame, where zero means unbounded. Such timeline
clips now fail before native initialization with structured unsupported capability
`subframe_timeline_clip`; see the precise serialized-duration boundary in
[linked-api.md](linked-api.md). This explicit restriction avoids hanging inside a
single filter call; it does not silently stretch audio or claim an unenforceable
deadline. Ordinary fractional clips above that boundary remain supported.

### 0.4.1 repository audit and source release preparation (2026-11-16)

The owner requested a new repository-wide bug/performance audit with two loops:
Gemini, Codex fixes, Copilot, Codex fixes. This task-specific sequence leaves the
global default and the five-report linked-migration history above unchanged.
The new task retains starting HEAD
`dd112d497b68df6786f49a71b3611f7f32f090e9`, original requirements, pre-existing
edits and one task ID in `/private/tmp/avid-core-two-loop-20261116-01a1e786`.
Only checked source/configuration/test/documentation text was handed off; media,
screenshots, generated output and unrelated audit evidence were excluded.

Gemini profile `gemini` selected CLI model `gemini-3.8-flash-high`, with fallback
disabled, but the CLI could not start its local listener or write its inherited
logs in the sandbox. Its resolver also logged that the requested ID was absent
from local configuration and defaulted to CCPA. No report or independent serving
model identity was obtained; this is not an accepted Gemini audit. The attempted
full snapshot was
`18f09e114a98eb739554f54eec3931a0da4e025b6e046ad7aeca36e05fd992f4`,
with text payload
`63659a037b03560cc44daafc94aa7b6df751b047645696fd555d9d522e66c395`.
Two snapshot tooling tests could not find excluded archived workflows; their
complete-checkout equivalents passed, and those text inputs were included in
the second review without enabling any workflow.

Codex removed two redundant clip-vector clone/revalidation passes from the
production renderer. A crate-private iterator exposes only mutable media paths;
clip IDs, order, durations and the cached timeline clock retain their validated
invariants. Three optimized benchmark processes measured 13.03–13.86% less time
for normalization of 1,024 clips, approximately 509 to 440 microseconds. Setup,
file checks, codecs, publication and final destruction were excluded; no
end-to-end export speedup is claimed. The manual benchmark and its scope are
documented in [verification](TESTING.md).

Copilot's existing read-only CLI adapter ran with fallback disabled after
automatic approval accepted the owner's task-specific override. The CLI used
its provider default; neither a selected model ID nor an independent serving
identity was exposed. Its completed report explicitly has partial coverage,
skims Android and documentation, and did not examine the supplied test results.
It does not establish a complete independent audit. Its full snapshot was
`13fac4769849461fe9bbb02bdfcfa6a64f69db97202a74b4a90c397d02f7edca`,
with text payload
`cf5aa5c89758bc77c429dcc982a4bf1b65e530bf4270fee06008a15d9e8ade4c`.
The reported low-severity URI-like basename restriction is intentional: callers
can explicitly qualify local `file:`/`content:` names with `./` or an absolute
path. The final Codex pass documents and tests that distinction without relaxing
URI rejection. It also regenerates the dependency inventory after the version
bump; the stale inventory failure in Copilot's validation remains recorded.
These later corrections are verified by Codex, not by that earlier report.

This remains a complete-workspace source-library release. Exact committed source
archive integrity and fresh extracted-source builds/tests are recorded in the
release receipt before publication. Compiled platform candidates, unavailable
native platform qualification, downstream compatibility and App Store rights
remain separate. No compiled unqualified artifact, macOS application, automatic
CI publication, consumer-pin change or restored archived workflow is implied.
