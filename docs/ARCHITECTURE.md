# Architecture

The safe Rust `avid-core` crate owns media policy, validation, settings/state,
composition graphs, output protection and publication. It forbids unsafe code.
`src/command.rs` constructs typed linked execution plans and canonical reusable
filter graphs; no production argument vector is interpreted as a CLI.
`src/renderer.rs` runs those plans through `avid-media-bridge`, maps genuine
library failures and manages progress, deadlines and fallback.
`src/operation.rs` owns cancellation independently of child processes.
`src/storage.rs` retains same-directory staging and alias protection.
`src/image/` retains the already linked JPEG/PNG optimization implementation.

The separately reviewable bridge owns C FFmpeg contexts for demuxing/decoding,
filtering, scaling/resampling, encoding, muxing and flushing. Narrow unsafe Rust
callbacks borrow synchronous lifetimes; no worker survives return. It does not
change global FFmpeg logging configuration. Production builds require a pinned
SDK with target identity; no runtime acquisition or system-library fallback.

`avid-core-ffi` owns exported ABI unsafety and opaque handle storage. It validates
foreign length-delimited schema JSON and dispatches to canonical libraries.
C/Swift/JNI examples are transport wrappers and retain no rendering policy.
Apple transcription engines remain Swift packages using their native runtimes;
portable transcript/model contracts and the separate pinned whisper.cpp adapter
live in `avid-transcription`. Provider/model capabilities remain explicit and do
not imply portable CoreML/MLX model files or forced alignment.

Historical `src/legacy_renderer.rs`, `src/media.rs`, `src/process.rs` and
`src/runtime.rs` compile only with `legacy-subprocess`. Historical command
construction stays test/reference-only. Production never selects it, including
linked initialization failure. The historical executable runtime contract is not
reused for linked artifacts.

The detailed [ABI/migration](linked-api.md), [artifact contract](linked-artifacts.md),
[transcription contract](linked-transcription.md), and [status matrix](linked-status.md)
define ownership, lifecycle and qualification limits. Existing project schema,
UI, permission dialogs, stores, application signing and host releases remain
outside Core.
