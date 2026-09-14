//! Canonical artwork-and-audio video engine. Run blocking operations on a host worker thread.
//! The crate owns no UI, process protocol, project archive, global job registry, or logging policy.
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]
mod command;
mod error;
mod media;
mod model;
mod presets;
mod process;
mod progress;
mod renderer;
mod state;
mod storage;

pub use error::{Error, ProcessFailure, Result};
pub use media::{MediaTools, ToolDiscovery};
pub use model::{
    select_clip_indices, Capabilities, Clip, Codec, Composition, EncoderCapability, Encoding,
    Input, PreviewRequest, RenderMode, RenderRequest, RenderSettings, Timeline, TimelinePosition,
};
pub use presets::{Preset, PRESETS};
pub use process::CancellationToken;
pub use progress::{EventSink, Progress, Stage};
pub use renderer::{OperationOptions, Renderer};
pub use state::{VideoProjectState, VideoSettings};

pub const MAX_SOURCE_IMAGE_DIMENSION: u32 = 32_768;
pub const MAX_SOURCE_IMAGE_PIXELS: u64 = 50_000_000;
pub const MAX_OUTPUT_DIMENSION: u32 = 8_192;
pub const MAX_OUTPUT_PIXELS: u64 = 33_177_600;
