//! Streaming RGB decode used by hosts and native validation.
use super::*;

/// Pixels remain borrowed for this callback only. Rows are RGB24 and may have
/// padding; `stride` gives their byte spacing. This decoder reports coded pixels
/// and timestamps; Renderer applies orientation during composition.
pub struct VideoFrame<'a> {
    pub pixels: &'a [u8],
    pub stride: usize,
    pub width: u32,
    pub height: u32,
    pub seconds: Option<f64>,
}
struct VideoCallback<'a> {
    callback: &'a mut dyn FnMut(VideoFrame<'_>) -> bool,
    panicked: bool,
}
unsafe extern "C" fn frame(
    opaque: *mut c_void,
    pixels: *const u8,
    length: usize,
    stride: c_int,
    width: c_int,
    height: c_int,
    seconds: f64,
) -> c_int {
    // SAFETY: C supplies a live RGB frame for this synchronous callback only.
    let state = unsafe { &mut *opaque.cast::<VideoCallback<'_>>() };
    if state.panicked {
        return 0;
    }
    match catch_unwind(AssertUnwindSafe(|| {
        (state.callback)(VideoFrame {
            pixels: unsafe { std::slice::from_raw_parts(pixels, length) },
            stride: stride as usize,
            width: width as u32,
            height: height as u32,
            seconds: seconds.is_finite().then_some(seconds),
        })
    })) {
        Ok(value) => i32::from(value),
        Err(_) => {
            state.panicked = true;
            0
        }
    }
}
/// Decode a local video/image without subprocesses, buffering only one coded
/// frame and one RGB frame. Returning false from the callback ends successfully.
pub fn decode_video(
    path: &Path,
    control: &Control<'_>,
    callback: &mut dyn FnMut(VideoFrame<'_>) -> bool,
) -> Result<()> {
    let path = path_string(path)?;
    let mut state = CallbackState {
        control,
        panicked: false,
    };
    let control = native_control(&mut state);
    let mut frames = VideoCallback {
        callback,
        panicked: false,
    };
    let mut error = NativeError::default();
    // SAFETY: C retains none of these pointers and releases all contexts before
    // returning, including when the host cancels, stops decoding or panics.
    let code = unsafe {
        avid_media_decode_video(
            path.as_ptr(),
            &control,
            frame,
            (&mut frames as *mut VideoCallback<'_>).cast(),
            &mut error,
        )
    };
    finish(code, error, state.panicked || frames.panicked)
}
unsafe extern "C" {
    fn avid_media_decode_video(
        path: *const c_char,
        control: *const NativeControl,
        callback: unsafe extern "C" fn(
            *mut c_void,
            *const u8,
            usize,
            c_int,
            c_int,
            c_int,
            f64,
        ) -> c_int,
        opaque: *mut c_void,
        error: *mut NativeError,
    ) -> c_int;
}
