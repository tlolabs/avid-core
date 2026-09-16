# Canonical shared crate extraction report

> Historical extraction document. FFmpeg acquisition, build ownership and migration instructions are superseded by [the Core-owned FFmpeg guide](ffmpeg/README.md) and [current host handoff](ffmpeg/migration.md).
The shared implementation is complete and independently verified in `/Users/tlothian/Documents/Projects/AVID Core`. The crate is `avid-core` version 0.1.0. **The complete ATIV feature engine is the baseline**, as clarified by the user; EnCAP Video's legitimate extensions are preserved alongside it. This phase did not migrate either application. The work is local, reviewable, and has not been pushed or released.

The two required standalone migration prompts are [ATIV](handoff-ativ.md) and [EnCAP Video](handoff-encap.md). Execute them in that order, with thorough ATIV verification before beginning EnCAP.

## 1. Architecture

One UI-independent Rust library owns artwork/audio-to-video processing. The public surface is an immutable `Renderer` configured with validated `MediaTools` and per-operation timeout options. Requests contain explicit execution settings, resolved media inputs and protected paths. Hosts supply cancellation and events. No shared operation knows which application is hosting it.

`Input::Single` preserves ATIV's original media path, including original audio sample rate/channel layout. `Input::Timeline` represents EnCAP's duration-trimmed hard-cut sequence. A single artwork graph builder supports fitted/sigma40 and square-padded/sigma20 treatments. A single process runner serves tool validation, capability queries, FFprobe, previews and encoding. One guarded staging/publication implementation covers preview, export and fallback.

The shared persisted state is limited to `VideoSettings` and `VideoProjectState`; whole-project persistence crosses EnCAP's three modes and stays in that host. Internal modules remain private. Explicit public models, errors, `EventSink` and a cloneable cancellation token provide the necessary adapter points without a universal-media abstraction layer. `select_clip_indices` lets EnCAP resolve selection before checked source mapping, avoiding a workaround involving fabricated unselected inputs.

## 2. Implementation comparison and reconciliation

The [inventory](inventory.md) maps corresponding functions, types, formats, dependencies, tests and host boundaries. Git history confirmed EnCAP's Video implementation adapted the earlier AVID feature in `86e0b33`; ATIV's native core originated in `f488f67` and was subsequently renamed. Neither source was treated as automatically authoritative.

| Difference | Shared behavior and reason |
|---|---|
| ATIV sigma40 + unpadded foreground; EnCAP sigma20 + square padding | Preserve both through explicit Composition values; artistic intent is not inferred from chronology |
| ATIV 1–240 fps and positive integer bitrate syntax; EnCAP 1–120 with preset bitrates | RenderSettings supports ATIV's full range; VideoSettings adapter preserves EnCAP's narrower existing policy |
| ATIV one original track; EnCAP selected/reordered chapters | Separate input semantics, shared graph/runner; avoid accidental normalization of ATIV audio |
| ATIV software H.264 only; EnCAP HEVC and GPU choices | Default remains ATIV software H.264; explicit typed codec/mode adds EnCAP capability detection and runtime fallback |
| ATIV rich progress, but uncancellable probes/preview; EnCAP cancellable execution but no progress protocol | One runner provides all-stage cancellation and progress; host protocols control event delivery |
| ATIV identity version check; EnCAP accepts any nonempty response | Validate expected version identity with a timeout; configured tool mismatch is reported |
| EnCAP searches app environment overrides; ATIV command-line overrides | Explicit injection; app-named environment behavior remains in adapters |
| ATIV timestamp staging guard; EnCAP releases tempfile ownership before encoding | Keep exclusive randomized staging ownership throughout encode/retry/publication; close cleanup holes |
| ATIV Unix inode comparison; EnCAP lexical audio-only comparison | Cross-platform same-file checks protect artwork/audio and extra host paths |
| Both lose some process diagnostics or retain unbounded output | Structured failure context, bounded 64 KiB stderr / 4 MiB stdout, explicit capture-limit errors |
| ATIV permits zero-sized probe dimensions through its upper-bound check | Reject zero dimensions, following stronger EnCAP behavior |
| EnCAP encoder parsing can match descriptions | Match actual video encoder name fields, avoiding false capability detection |
| Whole EnCAP project validation couples multiple modes | Host continues whole-project validation; shared renderer owns video-only rules |

No platform UI, Audio export, transcript system, ZIP schema implementation or unrelated cleanup was imported. No shared feature is synchronized between source copies. The existing hosts still contain their originals until the separate migration tasks remove them.

## 3. Shared functionality now owned by avid-core

- All 27 presets and their ordering/labels/dimensions; explicit fps field.
- Output settings, image safety limits, FPS/bitrate validation, typed codecs and encoding modes.
- Local image and audio duration probing, executable discovery and version validation.
- ATIV PNG previews, artwork composition and both flips using the export graph.
- ATIV single-track software H.264/AAC MP4 with yuv420p, faststart and end-at-audio behavior.
- EnCAP ordered clip selection, total duration/boundary lookup, artwork-per-clip, duration trims, hard concatenation and stereo/48 kHz normalization.
- H.264/HEVC encoder detection/selection, hvc1 tag, explicit hardware failure and automatic software fallback.
- Per-operation background-worker execution support, progress/ETA, diagnostics, cancellation, timeouts and bounded captures.
- Alias checks, protected host paths, staged file cleanup, flushed output and replacement.
- Video-specific JSON state/defaults, unknown fields, selection initialization and opaque future compositions.

## 4. Host-specific responsibilities

ATIV keeps its native windows/menus/theme, pickers, drop handlers, preview display and stale-request handling, async engine clients, CLI and NDJSON events, cancellation stdin protocol/exit code, app version, log policy and bundled distribution. Its thin adapter must explicitly retain software H.264 and fitted composition.

EnCAP keeps project/archive loading/saving/migration and recovery, episode/chapter/audio/transcript metadata, checked chapter-number-to-source mapping, main-artwork workflow requirements, Audio canonical ordering, mode navigation, playback devices and preview pause/seek, native UI, one-JSON-value engine protocol, signal handling and packaging. Video's adapter selects square-padded composition and preserves project validation and unknown-field merging. `encap-ffmpeg` stays for unrelated Audio/Transcript processing.

Both hosts own localization/presentation and same-destination job serialization. Neither host should retain a second video filter graph, preset table, progress parser, image validator, encoder selector or video subprocess/staging implementation after migration.

## 5. FFmpeg architecture

External executables remain the media stack. The user explicitly requires one FFmpeg build: the crate builds/bundles none, uses a single ffmpeg/ffprobe pair for both render paths, and rejects differing version identifiers. All EnCAP modes must consume the same resolved host pair. Both repositories pin 9.0.1; EnCAP's current custom macOS build lacks libx264/libx265, while ATIV's existing 9.0.1 pair includes them. The host migration must consolidate one build with all required capabilities, not add a second Video-specific version. Commands use OS-string arguments directly, stdin is disabled, local inputs are absolute paths, and input protocols are limited to `file,pipe`. Image/audio stdout is parsed after a bounded capture. A concurrently draining stdout/stderr runner avoids full-pipe deadlocks; a bounded event channel allows progress to coalesce instead of stalling FFmpeg.

Cancellation and timeout kill/reap the owned child. Timeout defaults are 30 seconds for probes/capabilities, 120 seconds for previews, and unlimited rendering; configuration can change these. Each encoding attempt has its own timeout. Automatic fallback only follows a media-process failure, not cancellation or timeout, and retains both failure causes if software also fails.

Output is reserved beside the target, retained through retries, flushed using a writable handle, then published with tempfile's platform overwrite implementation. Normal failure paths clean partial stages. Unix creation permissions match the source applications' normal FFmpeg file mode and respect umask without changing global state. Windows uses an established overwrite move rather than maintaining another hand-written backup sequence.

Unrelated MP3/AAC podcast metadata export, transcript audio extraction, optional encoders/transcribers, bundled binary signing and installation remain EnCAP/host-owned. The crate neither bundles FFmpeg nor introduces linking/bindings. Existing implicit FFmpeg input-metadata behavior is retained; no new explicit podcast metadata, chapter metadata or caption mapping is added.

## 6. Compatibility

ATIV has no saved project format. Its behavior and engine protocol are documented independently of the crate API. The handoff preserves its JSON messages/codes and application version, including adapter handling for new diagnostic error categories. MP4 output remains independent of a caller's filename suffix on the ATIV path, while timeline output enforces EnCAP's .mp4 convention.

EnCAP's schema-2 ZIP and schema-1 migration remain unchanged. Video state remains schema 1 with identical known fields/defaults and unknown extension preservation. `preview_quality` and future `compositions` survive round trips. Newer video schemas can deserialize losslessly but are rejected by `validate_schema()` before execution. Host archive normalization continues to remove stale selected IDs and initialize all only for an uninitialized selection. Shared execution rejects unresolved or duplicated selection IDs.

No filename, project directory structure, sidecar, cache or preference format is replaced. The internal staging prefix changes to `.avid-`; host cleanup tests must recognize it. No large source assets are moved. The full source inventory covers project metadata/persistence even though these systems properly remain EnCAP-owned.

## 7. Testing

Final native results: **36 unit/compatibility/lifecycle tests passed, one compiled documentation example passed, and all three explicit FFmpeg tests passed**. Default tests report the three FFmpeg tests as ignored; the dedicated invocation runs and passes them.

| Suite | Count | Coverage |
|---|---:|---|
| Library unit tests | 17 | Exact ATIV and EnCAP graphs; encoding flags/capabilities; progress units/ETA; FFprobe parsing/limits; discovery overrides; kill/reap/timeout; bounded capture/flooding; publication, permissions and hard-link protection |
| `tests/compatibility.rs` | 9 | Entire preset table; ATIV settings range; selection order/empty/missing/duplicate behavior; boundary timing; duration validation; JSON defaults, unknown fields, future compositions/schema handling; selection before host mapping |
| `tests/lifecycle.rs` | 10 | Successful fallback/progress; both fallback errors; explicit hardware failure; probe/preview/render timeout; in-flight/prepublication cancellation; preview and extra-source protection; cleanup; concurrent isolated jobs |
| Documentation | 1 | README public API example compiles |
| `tests/ffmpeg.rs` | 3 | Real PNG/H264/AAC, probe/progress and original audio format; sequence order/color/timing/stereo48k; HEVC hvc1 |

Fixtures are small checked-in JSON and one reference filter graph. Real-media tests generate their own short sine-wave WAV and colored PNG files. No FFmpeg is needed by default tests. Fake executable tests are Unix-only; portable validation/storage/state tests compile and are included in the Windows CI job.

Additional direct reference audit: compiled ATIV's unchanged original library into the shared repository's ignored target directory. Four landscape/portrait/square/flip preview cases produced **identical decoded pixels**. A complete reference export produced **identical decoded video and audio**, and duration probing matched. EnCAP's graph fixture was generated by executing its unchanged graph function, and the shared sequence graph matched exactly. See [reference-audit.json](reference-audit.json).

## 8. Build verification

| Command/check | Result |
|---|---|
| `cargo fmt --all -- --check` | Passed |
| `cargo check --locked --all-targets` | Passed |
| `cargo test --locked` | 36 tests + 1 doctest passed; 3 media tests intentionally ignored in this invocation |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed without suppressed warnings |
| `cargo +1.85.0 check --locked --all-targets` | Passed against declared minimum Rust |
| `cargo test --locked --test ffmpeg -- --ignored` | 3 passed |
| Windows x86_64 GNU all-target compile check | Passed |
| Linux x86_64 GNU all-target compile check | Passed |
| Source-content/HEAD/status audit | Passed, all 63 ATIV and 108 EnCAP enumerated files unchanged |
| Source remote HEAD comparison | Both match analyzed local HEADs |

Native checks used macOS Apple Silicon and Rust 1.98.1. Initial media checks used the already-installed FFmpeg/ffprobe 7.0.2-tessus; all three media tests and the direct ATIV comparison were then repeated successfully with the already-existing ATIV 9.0.1 arm64 pair. No FFmpeg was installed, downloaded, rebuilt, or copied into the shared repository. The 1.85 toolchain was separately installed and checked. A GitHub CI workflow is prepared for macOS/Windows/Linux default tests and minimum Rust; remote CI has not run because the work is not pushed. Real-media tests must also run in host packaging CI with the one approved FFmpeg build. The shared CI deliberately contains no second FFmpeg download/build job. The two runnable examples compile. No host manifests, source files, build systems or commits were changed.

## 9. Final cross-repository review and remaining risks

The final comparison revisited every ATIV core function and the EnCAP Video pipeline/state model, their engine protocols, native playback boundary, source history and test expectations. It corrected a Windows flush-handle issue, normal-file output mode drift, and a selection API that would otherwise make EnCAP map unselected records unnecessarily. Complete preset equality and exact graph fixtures protect against missed options or style drift. EnCAP project/archive functions were intentionally excluded because they span unrelated modes; Video state itself was retained and tested.

Remaining verification belongs in host migration/release work:

- Windows/Linux were compile-checked, not run on native systems here. Native overwrite/file locking, bundled paths, MSVC/ARM64 packages and platform UI behavior still need their normal release verification.
- EnCAP's current macOS custom FFmpeg recipe does not enable the software video encoders its Video settings expose. Consolidating the one approved build with libx264/libx265 and all unrelated mode requirements is a host-packaging migration prerequisite; do not solve it by adding a second pair.
- Hardware fallback behavior is tested deterministically with fake executables. The three real tests use software encoders; no GPU backend matrix was exercised. Advertised VAAPI or other encoders may require device configuration that neither source supplies.
- Native EnCAP preview is an approximate live UI composition, not pixel-identical to export; its existing blur/scale behavior and playback controls remain host-owned. Export pause/resume does not exist in the sources or shared crate.
- Neither host has consumed the crate yet. EnCAP full archive/native round trips must be rerun during integration even though the feature state is structurally preserved and tested here. The host handoffs explicitly retain compatibility payload merging and source mapping.
- Completion means successful publication, not durable recovery from machine power loss. No directory-fsync guarantee or transactional multi-process writer coordination is claimed. Hosts must serialize the same output target. Force-killing the whole process can leave stages; normal error/cancel/timeout paths clean them.
- Tool executables are trusted FFmpeg-family programs. The runner manages its owned child, not arbitrary descendant process trees started by unrelated wrapper scripts. Callbacks must not block indefinitely or panic.
- Media tests use small deterministic inputs and one local FFmpeg build; exhaustive malformed media, codecs, very long files, remote filesystems and every platform architecture remain outside this local validation.

The shared-crate phase is ready for integration. The next authorized task should migrate ATIV using Handoff A, verify it thoroughly, then migrate EnCAP Video using Handoff B and verify the whole application. The overall two-host migration is not complete.
