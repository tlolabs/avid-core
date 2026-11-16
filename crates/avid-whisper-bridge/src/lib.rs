//! Narrow, synchronous ownership boundary for the pinned whisper.cpp C API.
//! Each `Model` owns one context, is never shared, and frees it deterministically.
//! Callbacks cannot unwind across C and cancellation is safe on SDK worker threads.
use std::{
    ffi::{c_char, c_int, c_void, CStr, CString},
    marker::PhantomData,
    path::Path,
    ptr::NonNull,
    rc::Rc,
};

/// SHA-256 of the exact validated native SDK receipt linked into this crate.
pub const BUILD_IDENTITY_SHA256: &str = env!("AVID_WHISPER_BUILD_IDENTITY_SHA256");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Validation,
    Io,
    WhisperStatus,
    Cancelled,
    Allocation,
    CppException,
    UnknownException,
    Poisoned,
}
impl ErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Validation => "validation",
            Self::Io => "io",
            Self::WhisperStatus => "whisper_status",
            Self::Cancelled => "cancelled",
            Self::Allocation => "allocation",
            Self::CppException => "cpp_exception",
            Self::UnknownException => "unknown_exception",
            Self::Poisoned => "poisoned_context",
        }
    }
}
#[derive(Debug)]
pub struct Error {
    pub code: i32,
    pub kind: ErrorKind,
    pub operation: &'static str,
    pub detail: String,
}
impl Error {
    fn input(operation: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code: -1,
            kind: ErrorKind::Validation,
            operation,
            detail: detail.into(),
        }
    }
    fn native(operation: &'static str, raw: &RawError) -> Self {
        let kind = match raw.kind {
            1 => ErrorKind::Validation,
            2 => ErrorKind::Io,
            4 => ErrorKind::Cancelled,
            5 => ErrorKind::Allocation,
            6 => ErrorKind::CppException,
            7 => ErrorKind::UnknownException,
            8 => ErrorKind::Poisoned,
            _ => ErrorKind::WhisperStatus,
        };
        let length = raw
            .detail
            .iter()
            .position(|v| *v == 0)
            .unwrap_or(raw.detail.len());
        let bytes: Vec<u8> = raw.detail[..length].iter().map(|v| *v as u8).collect();
        Self {
            code: raw.code,
            kind,
            operation,
            detail: String::from_utf8_lossy(&bytes).into_owned(),
        }
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({} / {}): {}",
            self.operation,
            self.kind.as_str(),
            self.code,
            self.detail
        )
    }
}
impl std::error::Error for Error {}
#[repr(C)]
struct RawError {
    code: c_int,
    kind: c_int,
    detail: [c_char; 512],
}
impl Default for RawError {
    fn default() -> Self {
        Self {
            code: 0,
            kind: 0,
            detail: [0; 512],
        }
    }
}
#[derive(Debug, Clone)]
pub struct Segment {
    pub text: String,
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub tokens: Vec<Token>,
}
#[derive(Debug, Clone)]
pub struct Token {
    pub bytes: Vec<u8>,
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub confidence: f32,
}
pub struct Control<'a> {
    pub cancelled: &'a (dyn Fn() -> bool + Sync),
}
#[repr(C)]
struct RawControl {
    opaque: *mut c_void,
    cancelled: extern "C" fn(*mut c_void) -> c_int,
}
struct Callback<'a> {
    control: &'a Control<'a>,
}
extern "C" fn cancelled(opaque: *mut c_void) -> c_int {
    // SAFETY: only invoked synchronously before the borrowed Callback leaves scope.
    let callback = unsafe { &*(opaque.cast::<Callback<'_>>()) };
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        (callback.control.cancelled)()
    }))
    .unwrap_or(true) as c_int
}
extern "C" {
    fn avid_whisper_open(
        path: *const c_char,
        control: *const RawControl,
        error: *mut RawError,
    ) -> *mut c_void;
    fn avid_whisper_free(context: *mut c_void);
    fn avid_whisper_recognize(
        context: *mut c_void,
        samples: *const f32,
        count: c_int,
        language: *const c_char,
        threads: c_int,
        control: *const RawControl,
        segment: extern "C" fn(*mut c_void, *const c_char, f64, f64) -> c_int,
        token: extern "C" fn(*mut c_void, *const c_char, f64, f64, f32) -> c_int,
        opaque: *mut c_void,
        error: *mut RawError,
    ) -> c_int;
}
pub struct Model {
    context: NonNull<c_void>,
    _not_send_sync: PhantomData<Rc<()>>,
}
impl Model {
    pub fn open(path: &Path, control: &Control<'_>) -> Result<Self, Error> {
        let path = CString::new(
            path.to_str()
                .ok_or_else(|| Error::input("load model", "model path is not UTF-8"))?,
        )
        .map_err(|_| Error::input("load model", "model path contains NUL"))?;
        let mut callback = Callback { control };
        let raw = RawControl {
            opaque: (&mut callback as *mut Callback<'_>).cast(),
            cancelled,
        };
        // SAFETY: C owns returned context; path and callbacks outlive this synchronous call.
        let mut error = RawError::default();
        let context = NonNull::new(unsafe { avid_whisper_open(path.as_ptr(), &raw, &mut error) })
            .ok_or_else(|| Error::native("load model", &error))?;
        Ok(Self {
            context,
            _not_send_sync: PhantomData,
        })
    }
    pub fn recognize(
        &mut self,
        samples: &[f32],
        language: Option<&str>,
        threads: u32,
        control: &Control<'_>,
    ) -> Result<Vec<Segment>, Error> {
        if samples.is_empty()
            || samples.len() > 16_000 * 30
            || samples.iter().any(|v| !v.is_finite())
            || !(1..=64).contains(&threads)
        {
            return Err(Error::input(
                "recognize",
                "expected at most 30 seconds of finite 16 kHz mono PCM and 1..64 threads",
            ));
        }
        let language = language
            .map(CString::new)
            .transpose()
            .map_err(|_| Error::input("recognize", "language contains NUL"))?;
        let mut callback = Callback { control };
        let raw = RawControl {
            opaque: (&mut callback as *mut Callback<'_>).cast(),
            cancelled,
        };
        let mut segments = Vec::<Segment>::new();
        extern "C" fn segment(
            opaque: *mut c_void,
            text: *const c_char,
            start: f64,
            end: f64,
        ) -> c_int {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // SAFETY: C guarantees a valid NUL string during the callback and Rust
                // passes its exclusive Vec as opaque for this synchronous invocation.
                let output = unsafe { &mut *opaque.cast::<Vec<Segment>>() };
                let text = match unsafe { CStr::from_ptr(text) }.to_str() {
                    Ok(v) => v,
                    Err(_) => return 1,
                };
                output.push(Segment {
                    text: text.into(),
                    start_seconds: start,
                    end_seconds: end,
                    tokens: Vec::new(),
                });
                0
            }))
            .unwrap_or(1)
        }
        extern "C" fn token(
            opaque: *mut c_void,
            text: *const c_char,
            start: f64,
            end: f64,
            confidence: f32,
        ) -> c_int {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // SAFETY: same borrowed callback ownership as the segment callback.
                let output = unsafe { &mut *opaque.cast::<Vec<Segment>>() };
                let bytes = unsafe { CStr::from_ptr(text) }.to_bytes().to_vec();
                let Some(segment) = output.last_mut() else {
                    return 1;
                };
                segment.tokens.push(Token {
                    bytes,
                    start_seconds: start,
                    end_seconds: end,
                    confidence,
                });
                0
            }))
            .unwrap_or(1)
        }
        // SAFETY: context is exclusively borrowed; slices, strings and callbacks remain
        // valid until all whisper work completes. C never stores them after return.
        let mut error = RawError::default();
        let status = unsafe {
            avid_whisper_recognize(
                self.context.as_ptr(),
                samples.as_ptr(),
                samples.len() as c_int,
                language.as_ref().map_or(std::ptr::null(), |v| v.as_ptr()),
                threads as c_int,
                &raw,
                segment,
                token,
                (&mut segments as *mut Vec<Segment>).cast(),
                &mut error,
            )
        };
        if status != 0 {
            return Err(Error::native("recognize", &error));
        }
        Ok(segments)
    }
}
impl Drop for Model {
    fn drop(&mut self) {
        // SAFETY: unique owned context; Model cannot be cloned or used after drop.
        unsafe {
            avid_whisper_free(self.context.as_ptr());
        }
    }
}
