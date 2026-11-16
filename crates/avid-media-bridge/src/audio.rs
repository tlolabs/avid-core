use super::*;

/// Borrowed, interleaved f32 PCM on the original container clock. The callback
/// must copy samples it needs after return. Initial track delay is real silence;
/// later discontinuities are rejected rather than silently shifting captions.
pub struct AudioChunk<'a> {
    pub samples: &'a [f32],
    pub channels: u32,
    pub start_seconds: f64,
}
struct AudioState<'a> {
    callback: &'a mut dyn FnMut(AudioChunk<'_>) -> bool,
    panicked: bool,
}
unsafe extern "C" fn audio_chunk(
    opaque: *mut c_void,
    samples: *const f32,
    count: usize,
    channels: c_int,
    start: f64,
) -> c_int {
    // SAFETY: C invokes synchronously, passing a live array of count*channels
    // initialized floats. The exclusive callback state is never used concurrently.
    let state = unsafe { &mut *opaque.cast::<AudioState<'_>>() };
    if state.panicked || !(1..=2).contains(&channels) || count > 1_048_576 || samples.is_null() {
        return 1;
    }
    let samples = unsafe { std::slice::from_raw_parts(samples, count * channels as usize) };
    match catch_unwind(AssertUnwindSafe(|| {
        (state.callback)(AudioChunk {
            samples,
            channels: channels as u32,
            start_seconds: start,
        })
    })) {
        Ok(keep_going) => i32::from(!keep_going),
        Err(_) => {
            state.panicked = true;
            1
        }
    }
}
/// Synchronously stream decoded/resampled audio; returning false from `callback`
/// aborts. No native pointer, worker or PCM allocation survives this call.
pub fn decode_audio(
    path: &Path,
    sample_rate: u32,
    channels: u32,
    control: &Control<'_>,
    callback: &mut dyn FnMut(AudioChunk<'_>) -> bool,
) -> Result<()> {
    if !(8_000..=192_000).contains(&sample_rate) || !(1..=2).contains(&channels) {
        return Err(invalid("invalid audio sample rate/channel request"));
    }
    let path = path_string(path)?;
    let mut state = CallbackState {
        control,
        panicked: false,
    };
    let native = native_control(&mut state);
    let mut audio = AudioState {
        callback,
        panicked: false,
    };
    let mut error = NativeError::default();
    // SAFETY: all pointers remain valid for this synchronous operation. Native
    // contexts and resampler buffers are freed before the call returns.
    let code = unsafe {
        avid_media_decode_audio(
            path.as_ptr(),
            sample_rate as c_int,
            channels as c_int,
            audio_chunk,
            (&mut audio as *mut AudioState<'_>).cast(),
            &native,
            &mut error,
        )
    };
    finish(code, error, state.panicked || audio.panicked)
}
unsafe extern "C" {
    fn avid_media_decode_audio(
        path: *const c_char,
        rate: c_int,
        channels: c_int,
        callback: unsafe extern "C" fn(*mut c_void, *const f32, usize, c_int, f64) -> c_int,
        opaque: *mut c_void,
        control: *const NativeControl,
        error: *mut NativeError,
    ) -> c_int;
}
