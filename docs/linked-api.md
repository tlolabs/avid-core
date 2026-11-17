# Linked API and host migration (0.4)

`avid-core` 0.4 replaces the production executable renderer with in-process
FFmpeg libraries. This is a pre-1.0 minor API break: replace
`Renderer::new(MediaTools::from_core_directory(...))` with `Renderer::new()`.
Do not ship `legacy-subprocess`; it exposes the explicitly named
`LegacyRenderer` and historical runtime management APIs only for reference tests.
The old constructor has not silently acquired different semantics. Persisted
`VideoSettings` and `VideoProjectState` retain their schema and defaults.

The safe Rust crate retains `#![forbid(unsafe_code)]`. The small
`avid-media-bridge` owns FFmpeg contexts and foreign callbacks; `avid-core-ffi`
owns the exported C boundary. Operations block the calling worker thread, use
per-operation contexts, and return only after cleanup. Independent jobs may run
concurrently. Hosts must serialize writers to the same output. Cancellation and
FFmpeg I/O interrupt callbacks are cooperative. A codec/driver call already in
progress can delay cancellation and deadlines; no detached worker or hard timeout
is promised. Core does not install a global FFmpeg log callback or change host
log levels. Driver messages may still use the host's FFmpeg logging policy.

Simple mode prepares each unique artwork sequentially using the canonical
composition graph and operation-owned PNG files. Its timeline graph caches
output-sized frames, avoiding full-resolution decode/scaling contexts per clip.
Original sources remain protected; caches are removed on failure or before final
publication. The first encoding attempt's deadline includes this preparation;
each artwork also respects the preview deadline. A fallback attempt reuses the
prepared files and receives the documented fresh encoding budget.

Timeline durations are serialized to microseconds for the canonical FFmpeg graph.
A duration that rounds to zero in the output frame time base is unsupported;
FFmpeg interprets that zero trim duration as unbounded. Core rejects it before
native initialization with capability `subframe_timeline_clip`. The minimum
serialized duration is `ceil(1_000_000 / (2 * fps))` microseconds: 20,834 at 24 fps,
16,667 at 30 fps and 8,334 at 60 fps. Positive durations above that boundary retain
the existing frame-quantized hard-cut behavior. Core does not stretch a clip or
claim a hard timeout can interrupt an infinite filter call.

## C ABI 1, schema 1

Use `include/avid_core.h` (C/C++, Swift module map) and `libavid_core_ffi`.
Every successful create/result owns one opaque integer handle; release it once.
A job retains references to its Core/token. Releasing a handle does not cancel a
job or make library unload safe: join all calls before unloading. Cancellation is
thread-safe. Result buffers remain owned by Core until release; query length and
copy to host memory. Strings use explicit UTF-8 byte lengths, with no terminal
NUL. The request maximum is 4 MiB. Empty/NUL paths, URI paths, unknown operations,
invalid typed values, malformed JSON and schema mismatches return structured
errors. Rust callers may use native `Path`; foreign paths use UTF-8, including
Windows Unicode paths, and must represent accessible local files.

For media requests, bare `file:` and `content:` prefixes are reserved URI spellings
(case-insensitive). This deliberately rejects ambiguous POSIX basenames such as
`file:1.png` and `content:name`, even when those files exist. To access such local
files, use an explicit relative path (`./file:1.png`, `./content:name`) or an
absolute filesystem path (`/imports/file:1.png`). This does not provide URI
resolution: import actual `file:` or `content:` resources through the host first.

The callback borrows JSON only until it returns and runs on the calling worker.
Copy before dispatching to the UI. Return promptly, never unwind across C, and do
not synchronously wait for the current job. A callback may cancel via its token.
No callback context is retained after execute returns. Rust layouts, pointers to
Rust objects, trait objects, and panics never cross the interface. Foreign memory
must be valid for its declared length; an ABI cannot prove arbitrary pointer
validity in a shared address space. Unknown ABI versions must be rejected before
calling; future incompatible schemas use another version.

Request examples:

```json
{"schema_version":1,"operation":"capabilities"}
```

```json
{"schema_version":1,"operation":"render","image":"/import/cover.png","audio":"/import/song.wav","output":"/exports/movie.mp4","mode":"simple","settings":{"width":1920,"height":1080,"fps":30,"codec":"h264","encoding":"automatic","composition":"fitted","audio_bitrate":"128k"}}
```

Other operations: `inspect_image` (`image`), `probe_audio` (`audio`), `preview`
(`image`, `output`, `settings`), `optimize_image` (`image`, `output`, `options`).
Timeline render supplies `clips:[{id,image,audio,duration_seconds}]` in playback
order. `protected_paths` applies to all outputs. `settings` is the Rust
`RenderSettings` schema, not the host's persisted `VideoSettings` schema; composition
is explicitly `fitted` or `square_padded`. Defaults preserve Rust settings. Explicit
software on an Apple profile without x264/x265 reports unsupported capability;
choose automatic for that profile, never rewrite a saved explicit software choice.
The distinct Apple GPL profile compiles both VideoToolbox and x264/x265 to support
Automatic hardware-to-software fallback where distribution rights permit it.
Software requests enumerate compiled encoders without initializing GPU drivers;
public capabilities and hardware/automatic jobs test hardware initialization.

Rust callers constructing `EncoderCapability` now supply `compiled`, `usable`
and optional `detail`; `Capabilities` also exposes `filters` and `linked_version`.
Compiled support alone cannot establish availability on the running device.
For a new project, use `Renderer::default_settings()` to select the profile's
working encoding default. Keep an existing project's explicit saved choice.

Results are `{schema_version:1,ok:true,result:...}` or
`{schema_version:1,ok:false,error:{code,detail}}`. Events have `event:stage|progress|diagnostic`.
`complete` means successful atomic publication, never encoder EOF alone. A zero
result handle means an ABI panic/allocation failure and must be treated as failure.
Native errors additionally carry `library`, `operation` and `native_code`;
Whisper bridge failures carry `native_kind` as well. Rust represents these as
`Error::Linked` for libav and `Error::NativeLibrary` for the inference runtime.
Fallback errors retain nested hardware and software errors with their original
domains. No subprocess record is invented for either native library.

## Host handoff

The [platform-by-platform ATIV/EnCAP handoff](linked-handoff.md) gives concrete
replacement steps and the native evidence still required for each host.

ATIV and EnCAP remain unchanged. Their maintainers must remove media-engine child
process launch/protocol code, link a verified target-specific Core artifact and
its listed transitive dependencies, translate requests once, and route callback
copies to their UI dispatcher. Preserve project protected paths and serialize
output writers. Keep original audio/artwork files available for the operation.
Use separate cancellation tokens per job and join workers on teardown. Retain
existing settings/state serialization and selection policy. Validate decoded
output and application behavior before choosing new application pins.

Swift hosts can use `examples/apple-linked/AVIDCore.swift`; C++/Qt hosts use the C
header and copy event data before queued Qt signals; Android uses
`examples/android-linked` JNI transport. No wrapper implements media composition.
Apple hosts must obtain security-scoped access and keep it valid or import files
into their sandbox first. A security-scoped URL/bookmark is not an ordinary
filesystem path. Android hosts must open `content://` through ContentResolver and
copy/import into accessible local storage. Core does not implement a host I/O
adapter or resolve either permission scheme. Hosts choose platform-appropriate
cache/export directories.

Transcription is a coordinated linked subsystem with its own provider/model
contracts; see [the transcription migration](linked-transcription.md). Replacing
ATIV's transcription helper means linking the appropriate package/adapter and
calling its provider directly, not wrapping that helper in another process.
Subtitle export does not add animated caption burn-in.
