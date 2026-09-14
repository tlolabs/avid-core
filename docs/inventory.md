# Extraction inventory and decisions

The user clarified that the core feature set is everything in ATIV. Its complete feature engine is the baseline; EnCAP sequence and encoding capabilities extend that baseline. UI/lifecycle stay in the host.

Reference heads are recorded in source-snapshot.json with content hashes and initial status. ATIV has pre-existing untracked assets; EnCAP is clean. Sources are read-only throughout this phase.

ATIV 2.0-era native core originated in f488f67 (2026-09-12); 3bc4e58 added native Windows/Linux, and 34dfccc renamed AVID to ATIV. EnCAP 86e0b33 (2026-09-12) explicitly adapted AVID into its Video mode. Chronology alone does not establish authority.

| Subsystem | ATIV reference | EnCAP reference | Canonical decision |
|---|---|---|---|
| Presets | ativ-core/model.rs Preset/PRESETS, 27 entries | encap-video VideoPreset/PRESETS, same entries plus fps=30 | One table, fps=30; ATIV protocol adapter omits extra field if needed |
| Single artwork/track | RenderRequest/render_video | One chapter is a special case of export | Explicit single-track input; preserve original audio layout/rate and end-at-audio behavior |
| Sequence | absent | selected_chapters, export_arguments, filter_graph | Clip timeline, ordered stable-ID selection, hard audiovisual concat; normalize audio to stereo/48kHz |
| Artwork | render::filter_graph: sigma40, fitted foreground | encap-video::filter_graph: sigma20, black square padding | Explicit Composition enum; one graph builder handles both |
| Preview | render_preview PNG through FFmpeg | native playback/composition in all clients | Shared cancellable PNG rendering and timeline lookup; playback/device/UI remain host |
| Flips | before split, both layers | same | One implementation |
| Dimensions/image limits | 8192 / 33,177,600 output; 32768 / 50M source | same, rejects zero source dimensions | Stronger zero check; common limits |
| FPS/bitrate | 1–240, positive digits with optional k/m/b | 1–120; 64/96/128/160/192/256/320k | Core supports union; persisted VideoSettings validator retains EnCAP policy |
| Codecs | libx264 software, AAC | H264/HEVC, automatic/hardware/software | Typed codec/mode, capability list and automatic fallback; single-track defaults software H264 |
| HEVC | absent | hvc1 tag | Preserve |
| Process | direct output() for probes/preview, pipe render monitor | tempfile stdout/stderr with 50ms polling | One pipe-draining runner, bounded capture, timeout, owned-child kill/reap |
| Progress | out_time_us/ms/timestamp, speed, ETA; named stages | none in export protocol | Shared EventSink and parser, host decides delivery protocol |
| Cancellation | Arc<AtomicBool>, only render | clone token, processes cancellable; capability query separate token | Clone token shared across all operation stages; worker-thread API, no async runtime |
| Discovery | override, adjacent, ffmpeg subdir, Resources, Frameworks, PATH | ENCAP_FFMPEG/FFPROBE, adjacent, Resources, PATH | Explicit override/search directories plus standard bundle/PATH search; environment mapping in host |
| Tool validation | version identity prefix | merely nonempty output | Validate identity with cancellable timeout |
| Duration probe | first finite positive stream/format duration | source import via encap-core, video uses chapter duration | Shared video audio duration probe; unrelated import normalization stays host |
| Image probe | CSV width x height | same | Shared parser and cancellable probe |
| Destination aliases | Unix inode check, other canonical path | absolute lexical audio-only comparison | Cross-platform same-file check; protect images/audio and host-supplied additional paths |
| Staging | timestamp name + drop guard | drops NamedTempFile before use; cleanup branches miss some failures | Retain randomized tempfile ownership through encode/retry/publication |
| Publication | rename; Windows recovery backup | rename plus recovery fallback and directory sync | tempfile persist overwrite primitive, sync staged bytes first; retain old output on prepublication error |
| Error | code/user_message, often discards stderr | EncapError::Message, logged stderr | Typed errors with operation, executable, arguments, exit status, bounded stderr, IO source; host formats UI |
| Concurrency | per-render child and native background process | same | Independent per-call state and staged file, share immutable renderer; same destination requires host serialization |
| Video persistence | no saved project | encap-core VideoSettings/VideoProjectState | Preserve exact JSON defaults/keys/extensions/compositions in shared feature-state types |
| Project/archive | absent | project.rs schema2 ZIP, migrations, private extraction, compatibility_payload | EnCAP-owned: spans Audio/Transcript/Video; adapter retains opaque fields and archive merge logic |
| Podcast/chapter metadata | no explicit metadata export | encap-audio ffmetadata; Video deliberately excludes it | Audio metadata/export stays EnCAP; do not introduce captions/MP4 chapters into Video |
| Preferences/cache | native appearance, logs, preview temp files | autosave, tools/transcript cache, preview quality | Host-owned; preview_quality roundtrips as persisted video field |
| Navigation/window/menu/lifecycle | SwiftUI, WinUI, GTK | same frameworks, different app flows | Host-owned, no UI dependencies |
| Tests | 3 core tests, script/test_engine_integration.sh | 3 video tests, cancellation test, project migration tests | Port behavioral expectations, extend process/file/format/real-media coverage; no large fixtures |

## Dependency audit

ATIV core has no external dependencies. EnCAP video depends on encap-core, encap-ffmpeg, serde, tempfile, and tracing (unused directly). Shared crate uses serde/serde_json for existing video state, tempfile for safe staging, same-file for cross-platform file identity. It does not depend on either host, tracing, UUID, ZIP, clap, reqwest, transcript providers, platform UI, or an async runtime. No Cargo feature matrix is warranted. Rust 1.85 baseline, edition 2021 to fit both hosts. Derived EnCAP code is GPL-3.0-only; ATIV is GPL-3.0-or-later, so combined crate uses GPL-3.0-only and retains provenance.

## Classification and observable gaps

Composition and validation differences are treated as compatibility choices; history does not prove artistic intent. Preserve both. ATIV deliberately disables GPU per acceptance matrix. EnCAP capability advertisement is not a hardware usability test: preserve automatic runtime fallback and explicit hardware failure. VAAPI may need a device/upload configuration the source does not supply; do not promise all listed backends work. Neither source implements export pause/resume; preview pause is native playback. EnCAP preview blur differs from export and depends on UI scale; do not claim pixel parity with native playback. Neither source owns a universal metadata/media framework.

## Boundary before implementation

Public Renderer over injected MediaTools; RenderRequest contains Input (single-track or Timeline), RenderSettings, output and additional protected paths. Clip maps resolved artwork/audio plus duration and stable ID; EnCAP chapter_number-to-source mapping and main-artwork requirement remain its adapter. Shared timeline owns selection/timing. Shared VideoSettings/VideoProjectState preserve feature-specific persistence only. Private process, command graph, validation, and publication modules. No source host changes in this phase.

## Single FFmpeg build constraint and packaging evidence

The user requires no second FFmpeg version. Both sources pin 9.0.1, but their build configurations differ. EnCAP script/build_ffmpeg.sh enables libmp3lame/AudioToolbox/VideoToolbox and does not enable libx264/libx265; the existing local 9.0.1 custom binary confirms those software video encoders are missing. ATIV's existing 9.0.1 arm64 distribution pair supplies them as well as libmp3lame/AAC. Consolidate one approved version/build with the required capability union per platform/architecture in the host migrations; do not ship a separate Video FFmpeg. Shared MediaTools has exactly one ffmpeg and one ffprobe path and rejects mismatched version identifiers. EnCAP's retained unrelated process wrapper must use these same resolved host executables. This phase built/downloaded no FFmpeg.
