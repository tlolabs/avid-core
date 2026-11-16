// SPDX-License-Identifier: GPL-3.0-or-later
//! The narrow unsafe boundary around per-operation FFmpeg contexts.
//!
//! Native calls never retain Rust pointers. Callbacks execute synchronously on
//! the calling thread, may be reentrant for independent operations, and are
//! bounded by the lifetime of `Control`. Every context is freed before return.
//! This crate changes no process-wide FFmpeg logging policy. Hosts supply a
//! pinned SDK at build time; no library or executable is acquired at runtime.

use std::{
    ffi::{c_char, c_int, c_void, CStr, CString},
    fmt,
    panic::{catch_unwind, AssertUnwindSafe},
    path::{Path, PathBuf},
};

/// A library error, with FFmpeg's negative error code and bounded diagnostic.
#[derive(Clone, Debug)]
pub struct Error {
    pub code: i32,
    pub operation: String,
    pub detail: String,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {} ({})", self.operation, self.detail, self.code)
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;
fn invalid(detail: impl Into<String>) -> Error {
    Error {
        code: -22,
        operation: "validate linked input".into(),
        detail: detail.into(),
    }
}

/// Cooperative callbacks. The bridge owns no worker and returns only after all
/// native contexts have been freed. Blocking codec/driver calls may delay
/// cancellation. A callback panic is contained and becomes a structured error.
pub struct Control<'a> {
    pub cancelled: &'a dyn Fn() -> bool,
    pub progress: &'a dyn Fn(f64),
}
struct CallbackState<'a> {
    control: &'a Control<'a>,
    panicked: bool,
}
unsafe extern "C" fn cancelled(opaque: *mut c_void) -> c_int {
    // SAFETY: native code calls synchronously, with the exact stack pointer
    // passed to that call; there are no native workers or retained callbacks.
    let state = unsafe { &mut *opaque.cast::<CallbackState<'_>>() };
    if state.panicked {
        return 1;
    }
    match catch_unwind(AssertUnwindSafe(|| (state.control.cancelled)())) {
        Ok(value) => i32::from(value),
        Err(_) => {
            state.panicked = true;
            1
        }
    }
}
unsafe extern "C" fn progress(opaque: *mut c_void, seconds: f64) {
    // SAFETY: same synchronous callback ownership as `cancelled`.
    let state = unsafe { &mut *opaque.cast::<CallbackState<'_>>() };
    if !state.panicked
        && catch_unwind(AssertUnwindSafe(|| (state.control.progress)(seconds))).is_err()
    {
        state.panicked = true;
    }
}
#[repr(C)]
struct NativeControl {
    opaque: *mut c_void,
    cancelled: unsafe extern "C" fn(*mut c_void) -> c_int,
    progress: unsafe extern "C" fn(*mut c_void, f64),
}
fn native_control(state: &mut CallbackState<'_>) -> NativeControl {
    NativeControl {
        opaque: (state as *mut CallbackState<'_>).cast(),
        cancelled,
        progress,
    }
}
#[repr(C)]
struct NativeError {
    code: c_int,
    operation: [c_char; 96],
    detail: [c_char; 2048],
}
impl Default for NativeError {
    fn default() -> Self {
        Self {
            code: 0,
            operation: [0; 96],
            detail: [0; 2048],
        }
    }
}
fn native_text(bytes: &[c_char]) -> String {
    let bytes: Vec<_> = bytes
        .iter()
        .take_while(|b| **b != 0)
        .map(|b| *b as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}
fn finish(code: c_int, error: NativeError, panicked: bool) -> Result<()> {
    if panicked {
        return Err(invalid(
            "A host callback panicked; operation resources were released",
        ));
    }
    if code < 0 {
        Err(Error {
            code,
            operation: native_text(&error.operation),
            detail: native_text(&error.detail),
        })
    } else {
        Ok(())
    }
}
fn path_string(path: &Path) -> Result<CString> {
    #[cfg(unix)]
    let bytes = {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str().as_bytes()
    };
    #[cfg(not(unix))]
    let bytes = path
        .to_str()
        .ok_or_else(|| invalid("Paths must be valid Unicode on this platform"))?
        .as_bytes();
    CString::new(bytes).map_err(|_| invalid("A filesystem path contains NUL"))
}
fn string(value: &str) -> Result<CString> {
    CString::new(value).map_err(|_| invalid("A plan string contains NUL"))
}

#[derive(Clone, Debug)]
pub struct Probe {
    pub width: u32,
    pub height: u32,
    pub duration: Option<f64>,
    pub has_audio: bool,
    pub has_video: bool,
    pub audio_rate: u32,
    pub audio_channels: u32,
    pub video_codec_id: i32,
    pub audio_codec_id: i32,
    pub video_start: Option<f64>,
    pub audio_start: Option<f64>,
    pub sample_aspect_ratio: (i32, i32),
}
#[repr(C)]
#[derive(Default)]
struct NativeProbe {
    width: c_int,
    height: c_int,
    has_audio: c_int,
    has_video: c_int,
    duration: f64,
    audio_rate: c_int,
    audio_channels: c_int,
    video_codec: c_int,
    audio_codec: c_int,
    video_start: f64,
    audio_start: f64,
    sar_num: c_int,
    sar_den: c_int,
}
pub fn probe(path: &Path, control: &Control<'_>) -> Result<Probe> {
    let path = path_string(path)?;
    let mut output = NativeProbe::default();
    let mut error = NativeError::default();
    let mut state = CallbackState {
        control,
        panicked: false,
    };
    let control = native_control(&mut state);
    // SAFETY: pointers reference live, correctly sized local storage, and C
    // retains none of them beyond this synchronous call.
    let code = unsafe { avid_media_probe(path.as_ptr(), &control, &mut output, &mut error) };
    finish(code, error, state.panicked)?;
    Ok(Probe {
        width: output.width.max(0) as u32,
        height: output.height.max(0) as u32,
        duration: (output.duration.is_finite() && output.duration > 0.0).then_some(output.duration),
        has_audio: output.has_audio != 0,
        has_video: output.has_video != 0,
        audio_rate: output.audio_rate.max(0) as u32,
        audio_channels: output.audio_channels.max(0) as u32,
        video_codec_id: output.video_codec,
        audio_codec_id: output.audio_codec,
        video_start: output.video_start.is_finite().then_some(output.video_start),
        audio_start: output.audio_start.is_finite().then_some(output.audio_start),
        sample_aspect_ratio: (output.sar_num, output.sar_den),
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaType {
    Video,
    Audio,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Repeat {
    None,
    Still,
    Gif,
    Apng,
}
#[derive(Clone, Debug)]
pub struct Input {
    pub path: PathBuf,
    pub media: MediaType,
    pub repeat: Repeat,
    pub fps: u32,
    pub duration: Option<f64>,
}
/// Typed canonical filter plan. Hosts should use avid-core's request API rather
/// than construct a plan: validation, staging and publication belong to Core.
#[derive(Clone, Debug)]
pub struct Plan {
    pub inputs: Vec<Input>,
    pub graph: String,
    pub video_label: String,
    pub audio_label: Option<String>,
    pub output: PathBuf,
    pub encoder: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub audio_bitrate: u64,
    pub duration: Option<f64>,
    pub preview: bool,
    /// Timeline exports normalize to stereo/48 kHz; single inputs preserve their sample rate/layout.
    pub normalize_audio: bool,
}
#[repr(C)]
struct NativeInput {
    path: *const c_char,
    media: c_int,
    repeat: c_int,
    fps: c_int,
    duration: f64,
}
#[repr(C)]
struct NativePlan {
    inputs: *const NativeInput,
    input_count: usize,
    graph: *const c_char,
    video_label: *const c_char,
    audio_label: *const c_char,
    output: *const c_char,
    encoder: *const c_char,
    width: c_int,
    height: c_int,
    fps: c_int,
    preview: c_int,
    normalize_audio: c_int,
    audio_bitrate: i64,
    duration: f64,
}
pub fn execute(plan: &Plan, control: &Control<'_>) -> Result<()> {
    if plan.graph.len() > 1_048_576 {
        return Err(invalid("Canonical graph exceeds one MiB"));
    }
    if plan.inputs.is_empty()
        || plan.inputs.len() > 2048
        || !(1..=240).contains(&plan.fps)
        || plan.width > 8192
        || plan.height > 8192
        || plan.width == 0
        || plan.height == 0
        || u64::from(plan.width) * u64::from(plan.height) > 33_177_600
        || plan.audio_bitrate > i64::MAX as u64
        || plan.duration.is_some_and(|d| !d.is_finite() || d <= 0.0)
        || plan.inputs.iter().any(|i| {
            !(1..=240).contains(&i.fps) || i.duration.is_some_and(|d| !d.is_finite() || d <= 0.0)
        })
    {
        return Err(invalid(
            "Invalid dimensions, fps, bitrate, input count or duration",
        ));
    }
    let paths: Vec<_> = plan
        .inputs
        .iter()
        .map(|i| path_string(&i.path))
        .collect::<Result<_>>()?;
    let inputs: Vec<_> = plan
        .inputs
        .iter()
        .zip(&paths)
        .map(|(i, path)| NativeInput {
            path: path.as_ptr(),
            media: if i.media == MediaType::Video { 0 } else { 1 },
            repeat: match i.repeat {
                Repeat::None => 0,
                Repeat::Still => 1,
                Repeat::Gif => 2,
                Repeat::Apng => 3,
            },
            fps: i.fps as i32,
            duration: i.duration.unwrap_or(f64::NAN),
        })
        .collect();
    let graph = string(&plan.graph)?;
    let video_label = string(&plan.video_label)?;
    let audio_label = plan.audio_label.as_deref().map(string).transpose()?;
    let output = path_string(&plan.output)?;
    let encoder = string(&plan.encoder)?;
    let native = NativePlan {
        inputs: inputs.as_ptr(),
        input_count: inputs.len(),
        graph: graph.as_ptr(),
        video_label: video_label.as_ptr(),
        audio_label: audio_label
            .as_ref()
            .map_or(std::ptr::null(), |s| s.as_ptr()),
        output: output.as_ptr(),
        encoder: encoder.as_ptr(),
        width: plan.width as i32,
        height: plan.height as i32,
        fps: plan.fps as i32,
        preview: i32::from(plan.preview),
        normalize_audio: i32::from(plan.normalize_audio),
        audio_bitrate: plan.audio_bitrate as i64,
        duration: plan.duration.unwrap_or(f64::NAN),
    };
    let mut error = NativeError::default();
    let mut state = CallbackState {
        control,
        panicked: false,
    };
    let control = native_control(&mut state);
    // SAFETY: all strings, input arrays, control, and error storage stay live
    // until C has synchronously destroyed every owned FFmpeg context.
    let code = unsafe { avid_media_execute(&native, &control, &mut error) };
    finish(code, error, state.panicked)
}
#[derive(Clone, Debug)]
pub struct Encoder {
    pub name: String,
    pub codec: String,
    pub hardware: bool,
    pub usable: bool,
    pub detail: Option<String>,
}
struct EncoderState {
    values: Vec<Encoder>,
    panicked: bool,
}
unsafe extern "C" fn add_encoder(
    opaque: *mut c_void,
    name: *const c_char,
    codec: *const c_char,
    hardware: c_int,
    usable: c_int,
    detail: *const c_char,
) {
    // SAFETY: C sends valid NUL-terminated borrowed strings during this call.
    let state = unsafe { &mut *opaque.cast::<EncoderState>() };
    if catch_unwind(AssertUnwindSafe(|| {
        state.values.push(Encoder {
            name: unsafe { CStr::from_ptr(name) }
                .to_string_lossy()
                .into_owned(),
            codec: unsafe { CStr::from_ptr(codec) }
                .to_string_lossy()
                .into_owned(),
            hardware: hardware != 0,
            usable: usable != 0,
            detail: Some(
                unsafe { CStr::from_ptr(detail) }
                    .to_string_lossy()
                    .into_owned(),
            ),
        });
    }))
    .is_err()
    {
        state.panicked = true;
    }
}
/// Enumerate compiled encoders and initialize hardware contexts to test current
/// hardware usability. Driver calls may block until their platform API returns.
pub fn encoders(control: &Control<'_>) -> Result<Vec<Encoder>> {
    encoder_list(control, true)
}
/// Enumerate compiled support without opening any codec or hardware context.
/// Hardware entries have `usable == false` and diagnostics explicitly state
/// that initialization was not probed. Software selection can use this path
/// without touching an unrelated GPU driver.
pub fn compiled_encoders(control: &Control<'_>) -> Result<Vec<Encoder>> {
    encoder_list(control, false)
}
fn encoder_list(control: &Control<'_>, probe_hardware: bool) -> Result<Vec<Encoder>> {
    let mut values = EncoderState {
        values: Vec::new(),
        panicked: false,
    };
    let mut state = CallbackState {
        control,
        panicked: false,
    };
    let control = native_control(&mut state);
    let mut error = NativeError::default();
    // SAFETY: the callback and all pointed-to state remain live synchronously.
    let code = unsafe {
        let enumerate = if probe_hardware {
            avid_media_encoders
        } else {
            avid_media_compiled_encoders
        };
        enumerate(
            add_encoder,
            (&mut values as *mut EncoderState).cast(),
            &control,
            &mut error,
        )
    };
    finish(code, error, state.panicked || values.panicked)?;
    Ok(values.values)
}
pub fn has_filter(name: &str) -> bool {
    let Ok(name) = string(name) else {
        return false;
    };
    // SAFETY: native code borrows the live NUL-terminated name only for this call.
    unsafe { avid_media_has_filter(name.as_ptr()) != 0 }
}
pub fn version() -> String {
    // SAFETY: FFmpeg returns an immutable static NUL-terminated string.
    unsafe { CStr::from_ptr(avid_media_version()) }
        .to_string_lossy()
        .into_owned()
}
pub fn configuration() -> String {
    // SAFETY: FFmpeg returns an immutable static NUL-terminated string.
    unsafe { CStr::from_ptr(avid_media_configuration()) }
        .to_string_lossy()
        .into_owned()
}
unsafe extern "C" {
    fn avid_media_probe(
        path: *const c_char,
        control: *const NativeControl,
        output: *mut NativeProbe,
        error: *mut NativeError,
    ) -> c_int;
    fn avid_media_execute(
        plan: *const NativePlan,
        control: *const NativeControl,
        error: *mut NativeError,
    ) -> c_int;
    fn avid_media_encoders(
        callback: unsafe extern "C" fn(
            *mut c_void,
            *const c_char,
            *const c_char,
            c_int,
            c_int,
            *const c_char,
        ),
        opaque: *mut c_void,
        control: *const NativeControl,
        error: *mut NativeError,
    ) -> c_int;
    fn avid_media_compiled_encoders(
        callback: unsafe extern "C" fn(
            *mut c_void,
            *const c_char,
            *const c_char,
            c_int,
            c_int,
            *const c_char,
        ),
        opaque: *mut c_void,
        control: *const NativeControl,
        error: *mut NativeError,
    ) -> c_int;
    fn avid_media_has_filter(name: *const c_char) -> c_int;
    fn avid_media_version() -> *const c_char;
    fn avid_media_configuration() -> *const c_char;
}

mod audio;
pub use audio::{decode_audio, AudioChunk};
mod video;
pub use video::{decode_video, VideoFrame};

#[cfg(test)]
mod encoder_tests {
    use super::*;
    #[test]
    fn compiled_enumeration_does_not_claim_hardware_initialization_and_honors_cancel() {
        let control = Control {
            cancelled: &|| false,
            progress: &|_| {},
        };
        let compiled = compiled_encoders(&control).unwrap();
        assert!(
            !compiled.is_empty(),
            "A linked media profile must provide video encoding"
        );
        for encoder in compiled {
            assert!(encoder.codec == "h264" || encoder.codec == "hevc");
            if encoder.hardware {
                assert!(!encoder.usable);
                assert!(encoder.detail.as_deref().unwrap().contains("not probed"));
            } else {
                assert!(encoder.usable);
                assert!(encoder
                    .detail
                    .as_deref()
                    .unwrap()
                    .contains("compiled software"));
            }
        }
        let interrupted = compiled_encoders(&Control {
            cancelled: &|| true,
            progress: &|_| {},
        })
        .unwrap_err();
        assert_eq!(interrupted.operation, "interrupt linked media operation");
        assert!(interrupted.code < 0);
    }
}
