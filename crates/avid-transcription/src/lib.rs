//! Portable transcript/caption contracts and verified offline model ownership.
//! Recognition is available only with the explicitly built `whisper` feature.
//! Existing Apple CoreML/MLX providers retain their separate native implementations.
#![forbid(unsafe_code)]
mod model;
mod transcript;
#[cfg(feature = "whisper")]
mod whisper;
pub use model::*;
pub use transcript::*;
#[cfg(feature = "whisper")]
pub use whisper::*;

#[derive(Debug)]
pub enum Error {
    InvalidInput(String),
    MissingWordTiming,
    Cancelled,
    Model(String),
    Busy,
    Io(std::io::Error),
    Inference(String),
    NativeMedia {
        code: i32,
        operation: String,
        detail: String,
    },
    NativeInference {
        code: i32,
        kind: &'static str,
        operation: &'static str,
        detail: String,
    },
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(v) | Self::Model(v) | Self::Inference(v) => f.write_str(v),
            Self::MissingWordTiming => {
                f.write_str("word timing is missing; explicit alignment is required")
            }
            Self::Cancelled => f.write_str("operation cancelled"),
            Self::Busy => f.write_str("model is in use"),
            Self::Io(v) => v.fmt(f),
            Self::NativeMedia {
                code,
                operation,
                detail,
            } => write!(f, "{operation} failed (libav {code}): {detail}"),
            Self::NativeInference {
                code,
                kind,
                operation,
                detail,
            } => write!(f, "{operation} failed ({kind}, {code}): {detail}"),
        }
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(v: std::io::Error) -> Self {
        Self::Io(v)
    }
}
pub type Result<T> = std::result::Result<T, Error>;
