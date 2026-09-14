# Handoff Prompt A — ATIV standalone migration

You are working in the ATIV project at `/Users/tlothian/Documents/Projects/ATIV` (remote `https://github.com/tlolabs/ativ.git`). Migrate ATIV to the independently built shared Rust crate `avid-core` at `/Users/tlothian/Documents/Projects/AVID Core` (remote `https://github.com/tlolabs/avid-core.git`). This is the first host migration. Do not migrate or modify EnCAP.

The extraction was based on ATIV commit `2c5eeed27e48d67fed128a62ff74dab6f00c2a82`. Reinspect current changes and preserve existing user work, including previously untracked `assets/`. The shared crate phase did not modify either host. Read the shared repository's README, `docs/report.md`, `docs/inventory.md`, and `docs/reference-audit.json` before implementation. Verify its current HEAD and review any changes since extraction. Use a `codex/` branch and logical commits. The shared crate is the authority; report and fix genuine shared gaps there rather than reimplementing the feature in ATIV. Coordinate any shared-crate changes separately from the host migration.

## Required outcome

Everything in ATIV's feature engine must continue working: all presets, artwork/audio input, dimension/image limits, PNG previews, both flips, software H.264/AAC video, 1–240 fps, existing bitrate syntax, progress/ETA, diagnostics, cancellation, bundled tool discovery and safe output replacement. Native application behavior and process protocol must remain compatible. Remove obsolete domain/media implementations only after their callers and tests use `avid-core`.

## One FFmpeg build

The user explicitly requires avoiding two FFmpeg versions. `avid-core` does not build or bundle FFmpeg. Use one matching ffmpeg/ffprobe pair from the approved 9.0.1 build per platform/architecture for both hosts' distribution inputs. Do not add a shared-crate-specific pair, mode-specific pair, sidecar FFmpeg distribution, or runtime version fallback. The shared discovery validator rejects differing version identifiers. Keep one build with the union of required capabilities (including libx264, libx265, AAC, and EnCAP's libmp3lame/AudioToolbox needs). Different video settings still use the same executable. Both existing repos pin 9.0.1; EnCAP's current macOS custom recipe lacks libx264/libx265, so coordinate its later packaging correction without modifying EnCAP in this task. ATIV's existing `build/ffmpeg-macos-arm64` pair passed all shared media tests and direct ATIV parity checks; validate all target-platform requirements before standardizing distribution artifacts.

## Dependency and boundaries

Add this at the ATIV workspace root:

```toml
[workspace.dependencies]
avid-core = { path = "../AVID Core" }
```

Use `avid-core.workspace = true` in the actual consuming member manifest. A direct path in `crates/ativ-core/Cargo.toml` is `../../../AVID Core`, not `../AVID Core`. During transition, `ativ-core` may remain a very thin protocol/API compatibility adapter. It must not retain filter graphs, subprocess orchestration, preset tables, progress parsing, media validation, or staging/publication implementations. Alternatively move the adapter to `ativ-engine` and retire `ativ-core` once every build/package reference is updated.

Keep native code under `platform/macos`, `platform/windows`, and `platform/linux` responsible for file pickers, windows, menus, drag/drop, appearance, async engine calls, accessibility, app lifecycle and preview display. Keep CLI parsing, JSON escaping/protocol serialization, `ATIV_LOG_PATH` and rotating local logging in `crates/ativ-engine/src/main.rs`. Do not introduce shared UI, an alternate media stack, FFmpeg bindings, synchronization scripts, or copied source files.

## Concrete replacement map

- `crates/ativ-core/src/model.rs::PRESETS/Preset` -> `avid_core::PRESETS/Preset`. The shared table has exactly the same 27 rows and order, plus `fps: 30`. Preserve ATIV's existing presets JSON shape; emit only its old fields if necessary. Native preset pickers must keep labels, grouping and selection behavior.
- `RenderRequest` -> map into `avid_core::RenderRequest { input: Input::Single { image, audio }, output, settings, protected_paths: vec![] }`. Copy width, height, fps, audio_bitrate and both flips. Set `Codec::H264`, `Encoding::Software`, `Composition::Fitted` explicitly. Do not use the sequence pipeline, which normalizes audio and trims duration. Do not use EnCAP's `VideoSettings` validator: that would narrow FPS and bitrate acceptance.
- `render::render_video` -> `Renderer::render`. `RenderSettings::default()` already matches ATIV (1920x1080, 30 fps, 128k, software H.264, fitted composition), but retain the existing CLI/native defaults and explicit inputs.
- `render::render_preview` -> `Renderer::preview` with `PreviewRequest` and `Composition::Fitted`. Both flips must apply to foreground and background. Preview output remains PNG irrespective of the target extension. Keep native preview-generation tokens so a stale async result cannot replace a newer image.
- `probe_audio_duration` -> `Renderer::probe_audio_duration`; unknown duration remains JSON null, not zero or an error. `Renderer::inspect_image` owns the source-dimension safety limits.
- `media::MediaTools/ToolOverrides` -> `MediaTools::discover(ToolDiscovery { ffmpeg, ffprobe, ..Default::default() }, &token)`. Preserve `--ffmpeg` and `--ffprobe`. Shared version fields are accessor methods (`ffmpeg_version()`, `ffprobe_version()`), not public struct fields. Standard search includes adjacent binaries, `ffmpeg/`, sibling Resources/Frameworks and PATH. Do not retain a second tool-discovery implementation.
- `Stage/RenderProgress/EventSink` -> `Stage/Progress/EventSink`. Stages retain the same `as_str()` values. Adapt `JsonEvents` to emit the existing JSON field names and retain local diagnostics. Log normalized progress from the callback if needed; the crate bounds diagnostic captures instead of retaining arbitrarily large raw process output.
- Existing `Arc<AtomicBool>` cancellation can be wrapped with `CancellationToken::from(Arc::clone(&cancelled))`; the shared token observes the existing flag. Alternatively use its cloneable token directly. Continue accepting case-insensitive `cancel\n` on stdin. Preserve cancellation exit code 130 and normal failure exit code 1. Thread cancellation through tool validation/probes/preview as well as rendering where the command supports it.
- Delete/retire local `filter_graph`, FFmpeg process monitor and parser, image/dimension/bitrate validators, output alias checks, `staging_path`, `StagedFile`, `publish`, and `clean_failure` after delegation is verified. Those belong to shared private modules.
- `ativ_core::VERSION` currently represents the application version. Keep engine/app version reporting tied to the ATIV package/workspace version, not shared crate version `0.1.0`.

## Error and process contract

ATIV 0.2 emits newline-delimited JSON events `tools`, `presets`, `probe`, `stage`, `progress`, `complete`, and `error`; clients ignore unknown fields/events. Preserve existing shapes, numeric null handling, and complete-kind messages. Do not send diagnostic text or file paths to stdout. Use native localized/plain-language messages in the adapter rather than exposing `avid_core::Error::Display`, which can contain paths and detailed stderr.

Keep existing machine codes for old categories: cancelled, invalid_input, media_tools_unavailable, media_tool_failed and io_error. Shared errors add timeout/capture_limit/fallback variants. For a strict existing protocol map Timeout/CaptureLimit/Fallback to media_tool_failed and retain the structured cause in local logs; introduce new codes only if all clients tolerate them and tests establish compatibility. Do not flatten the original process/OS error before logging. New cancellation checks and rejection of zero-sized artwork are intentional correctness improvements, not reasons to re-add the old paths.

The shared APIs block their calling worker thread. Preserve the current native asynchronous engine execution in Swift EngineClient, C# EngineClient and the GTK GSubprocess flow. Never run them on the UI thread. Only `Stage::Complete` means output publication succeeded. A progress fraction of 1 can precede publication. Callbacks must return promptly. UI stop after a completed export cannot undo a file.

## Verification and deletion gate

1. Establish the current ATIV build/test baseline and inventory any user changes. Keep reference files/media outside tracked host source changes when possible.
2. Run shared `cargo fmt --all -- --check`, `cargo check --locked --all-targets`, `cargo test --locked --all-targets`, `cargo clippy --locked --all-targets -- -D warnings`, and `cargo test --locked --test ffmpeg -- --ignored` with FFmpeg/ffprobe installed.
3. Migrate adapter and engine tests before removing obsolete code. Protect exact event JSON, error-code mapping, cancel exit status, presets order, version identity, command flags, and legacy positive bitrate syntax including 224k and integer bitrates. Keep 240 fps support.
4. Run ATIV workspace format/check/test/clippy and `script/test_engine_integration.sh`; adapt the script to the migrated engine where necessary without weakening its assertions. Strengthen its staging cleanup check to include `.avid-*` (the old `.ativ-*.tmp.*` pattern did not match all old staging files).
5. Generate small deterministic artwork and WAV files with spaces/Unicode in paths. Verify preview dimensions, flips, landscape/portrait/square/4:3/4:5 presets, H.264/AAC, yuv420p, faststart, mono versus stereo behavior, rate preservation and end-at-audio timing. Extraction already found identical decoded preview pixels for four orientation/flip cases and identical decoded video/audio for an ATIV reference export; use that as a baseline, not a substitute for the migrated engine tests.
6. Verify cancellation before spawn, during encoding, and just before publication; invalid/corrupt media; missing/wrong tools; existing-output preservation; hard-link/symlink source aliases; preview destination aliases; cleanup after failure and timeout. Keep source media unmodified.
7. Build and exercise macOS, Windows and Linux app packaging according to `docs/building.md`, `docs/acceptance-matrix.md`, and release scripts. The baseline includes macOS 12+ Apple Silicon/Intel, Windows 10 1809+ x64/ARM64 and GTK Linux. Verify bundled helper discovery, platform error presentation, cancellation, paths and output replacement on each platform. Cross-compile success alone is not runtime verification.
8. Remove unused domain files/dependencies and update build manifests, documentation and notices after verification. Search for `gblur`, `libx264`, `-filter_complex`, FFprobe command construction and duplicate preset tables; any remaining feature implementation needs a documented host-specific reason. Keep packaging tool lists and UI/protocol adapters as appropriate.

Do not migrate EnCAP or claim the entire consolidation complete. Deliver the ATIV migration diff/commits, the adapter decisions, tests actually run, any platform/manual gaps, and the exact shared revision tested. The next phase is thorough ATIV verification, then the separate EnCAP Video migration.
