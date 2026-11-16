//! Versioned foreign host boundary. Media behavior remains in avid-core.
//! Safety: foreign spans must designate readable/writable memory for their length;
//! callback pointers must obey the C header contract. No Rust object pointer escapes.
mod transcription;
use avid_core::*;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    ffi::c_void,
    panic::{catch_unwind, AssertUnwindSafe},
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
};

pub const ABI_VERSION: u32 = 1;
const MAX_REQUEST: usize = 4 * 1024 * 1024;
static NEXT: AtomicU64 = AtomicU64::new(1);
enum Handle {
    Core(Renderer),
    Cancel(CancellationToken),
    Bytes(Arc<Vec<u8>>),
}
fn handles() -> &'static Mutex<HashMap<u64, Handle>> {
    static H: OnceLock<Mutex<HashMap<u64, Handle>>> = OnceLock::new();
    H.get_or_init(|| Mutex::new(HashMap::new()))
}
fn insert(h: Handle) -> u64 {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    handles()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert(id, h);
    id
}
fn boundary<T>(fallback: T, call: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(call)).unwrap_or(fallback)
}
#[no_mangle]
pub extern "C" fn avid_core_abi_version() -> u32 {
    ABI_VERSION
}
#[no_mangle]
pub extern "C" fn avid_core_create() -> u64 {
    boundary(0, || insert(Handle::Core(Renderer::new())))
}
#[no_mangle]
pub extern "C" fn avid_core_cancel_create() -> u64 {
    boundary(0, || insert(Handle::Cancel(CancellationToken::default())))
}
#[no_mangle]
pub extern "C" fn avid_core_release(id: u64) -> i32 {
    boundary(-1, || {
        if handles()
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&id)
            .is_some()
        {
            0
        } else {
            -1
        }
    })
}
#[no_mangle]
pub extern "C" fn avid_core_cancel(id: u64) -> i32 {
    boundary(-1, || {
        let h = handles().lock().unwrap_or_else(|p| p.into_inner());
        if let Some(Handle::Cancel(c)) = h.get(&id) {
            c.cancel();
            0
        } else {
            -1
        }
    })
}
#[no_mangle]
pub extern "C" fn avid_core_buffer_len(id: u64) -> usize {
    boundary(0, || {
        let h = handles().lock().unwrap_or_else(|p| p.into_inner());
        if let Some(Handle::Bytes(b)) = h.get(&id) {
            b.len()
        } else {
            0
        }
    })
}
/// Copies into a host allocation; returns bytes copied, or zero for invalid/short spans.
/// # Safety
/// `out` must be writable for `capacity` bytes and may not overlap Core storage.
#[no_mangle]
pub unsafe extern "C" fn avid_core_buffer_copy(id: u64, out: *mut u8, capacity: usize) -> usize {
    boundary(0, || {
        if out.is_null() {
            return 0;
        }
        let b = {
            let h = handles().lock().unwrap_or_else(|p| p.into_inner());
            match h.get(&id) {
                Some(Handle::Bytes(b)) => b.clone(),
                _ => return 0,
            }
        };
        if capacity < b.len() {
            return 0;
        }
        unsafe {
            std::ptr::copy_nonoverlapping(b.as_ptr(), out, b.len());
        }
        b.len()
    })
}

pub type EventCallback = Option<unsafe extern "C" fn(*mut c_void, *const u8, usize)>;
struct Events {
    callback: EventCallback,
    context: usize,
}
// Calls occur synchronously only on the caller's thread; foreign context is opaque.
impl Events {
    fn send(&self, v: Value) {
        if let Some(callback) = self.callback {
            let data = v.to_string();
            unsafe {
                callback(self.context as *mut c_void, data.as_ptr(), data.len());
            }
        }
    }
}
impl EventSink for Events {
    fn stage(&self, s: Stage) {
        self.send(json!({"event":"stage","stage":s.as_str()}));
    }
    fn progress(&self, p: Progress) {
        self.send(json!({"event":"progress","elapsed_seconds":p.elapsed_seconds,"duration_seconds":p.duration_seconds,"fraction":p.fraction,"eta_seconds":p.eta_seconds}));
    }
    fn diagnostic(&self, s: &str) {
        self.send(json!({"event":"diagnostic","detail":s}));
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    schema_version: u32,
    operation: String,
    #[serde(default)]
    image: Option<String>,
    #[serde(default)]
    audio: Option<String>,
    #[serde(default)]
    output: Option<String>,
    #[serde(default)]
    clips: Vec<ClipRequest>,
    #[serde(default)]
    settings: Option<RenderSettings>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    protected_paths: Vec<String>,
    #[serde(default)]
    options: ImageOptimizationOptions,
    #[serde(default)]
    timeouts: Timeouts,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Timeouts {
    probe_ms: Option<u64>,
    preview_ms: Option<u64>,
    render_ms: Option<u64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClipRequest {
    id: String,
    image: String,
    audio: String,
    duration_seconds: f64,
}
fn path(v: Option<String>, name: &str) -> Result<PathBuf> {
    let s = v.ok_or_else(|| Error::InvalidInput(format!("Missing {name}")))?;
    if s.is_empty() || s.contains('\0') || s.contains("://") {
        return Err(Error::InvalidInput(format!(
            "{name} must be an accessible local filesystem path"
        )));
    }
    Ok(s.into())
}
fn execute(
    core: &Renderer,
    r: Request,
    cancel: &CancellationToken,
    events: &Events,
) -> Result<Value> {
    if r.schema_version != ABI_VERSION {
        return Err(Error::InvalidInput("Unsupported schema_version".into()));
    }
    let core = core.clone().with_options(OperationOptions {
        probe_timeout: Some(std::time::Duration::from_millis(
            r.timeouts.probe_ms.unwrap_or(30_000),
        )),
        preview_timeout: Some(std::time::Duration::from_millis(
            r.timeouts.preview_ms.unwrap_or(120_000),
        )),
        render_timeout: r.timeouts.render_ms.map(std::time::Duration::from_millis),
    });
    let protected = r
        .protected_paths
        .into_iter()
        .map(|s| path(Some(s), "protected_path"))
        .collect::<Result<Vec<_>>>()?;
    match r.operation.as_str() {
        "capabilities" => Ok(serde_json::to_value(core.capabilities(cancel)?).unwrap()),
        "inspect_image" => {
            let (width, height) = core.inspect_image(&path(r.image, "image")?, cancel)?;
            Ok(json!({"width":width,"height":height}))
        }
        "probe_audio" => Ok(
            json!({"duration_seconds":core.probe_audio_duration(&path(r.audio,"audio")?,cancel)?}),
        ),
        "preview" => {
            let output = core.preview(
                &PreviewRequest {
                    image: path(r.image, "image")?,
                    output: path(r.output, "output")?,
                    settings: r.settings.unwrap_or_else(|| core.default_settings()),
                    protected_paths: protected,
                },
                cancel,
                events,
            )?;
            Ok(json!({"output":output}))
        }
        "render" => {
            let input = if r.clips.is_empty() {
                Input::Single {
                    image: path(r.image, "image")?,
                    audio: path(r.audio, "audio")?,
                }
            } else {
                Input::Timeline(Timeline::new(
                    r.clips
                        .into_iter()
                        .map(|c| {
                            Ok(Clip {
                                id: c.id,
                                image: path(Some(c.image), "image")?,
                                audio: path(Some(c.audio), "audio")?,
                                duration_seconds: c.duration_seconds,
                            })
                        })
                        .collect::<Result<_>>()?,
                )?)
            };
            let mode = match r.mode.as_deref() {
                None | Some("per_frame") => RenderMode::PerFrame,
                Some("simple") => RenderMode::Simple,
                _ => return Err(Error::InvalidInput("Unknown render mode".into())),
            };
            let output = core.render_with_mode(
                &RenderRequest {
                    input,
                    settings: r.settings.unwrap_or_else(|| core.default_settings()),
                    output: path(r.output, "output")?,
                    protected_paths: protected,
                },
                mode,
                cancel,
                events,
            )?;
            Ok(json!({"output":output}))
        }
        "optimize_image" => {
            let value = optimize_image(
                &ImageOptimizationRequest {
                    input: path(r.image, "image")?,
                    output: path(r.output, "output")?,
                    options: r.options,
                    protected_paths: protected,
                },
                cancel,
            )?;
            Ok(
                json!({"input_bytes":value.input_bytes,"output_bytes":value.output_bytes,"bytes_saved":value.bytes_saved,"optimized":value.optimized}),
            )
        }
        _ => Err(Error::Unsupported {
            capability: r.operation,
            detail: "Unknown operation".into(),
        }),
    }
}
fn error_json(error: &Error) -> Value {
    let mut v = json!({"code":error.code(),"detail":error.to_string()});
    match error {
        Error::Linked {
            operation, code, ..
        } => {
            v["operation"] = json!(operation);
            v["library"] = json!("libav");
            v["native_code"] = json!(code);
        }
        Error::NativeLibrary {
            library,
            operation,
            code,
            kind,
            ..
        } => {
            v["library"] = json!(library);
            v["operation"] = json!(operation);
            v["native_code"] = json!(code);
            v["native_kind"] = json!(kind);
        }
        Error::Fallback { hardware, software } => {
            v["hardware"] = error_json(hardware);
            v["software"] = error_json(software);
        }
        _ => {}
    }
    v
}
/// Runs synchronously. Returned handle owns UTF-8 JSON; release it after copying.
/// # Safety
/// `request` must be readable for `length` bytes until return. Callback and context
/// must remain valid until return; the callback must not unwind or block indefinitely.
#[no_mangle]
pub unsafe extern "C" fn avid_core_execute_json(
    core: u64,
    cancel: u64,
    request: *const u8,
    length: usize,
    callback: EventCallback,
    context: *mut c_void,
) -> u64 {
    boundary(0, || {
        let result = (|| -> Result<Value> {
            if request.is_null() || length == 0 || length > MAX_REQUEST {
                return Err(Error::InvalidInput("Invalid request span".into()));
            }
            let (core, cancel) = {
                let h = handles().lock().unwrap_or_else(|p| p.into_inner());
                let core = match h.get(&core) {
                    Some(Handle::Core(c)) => c.clone(),
                    _ => return Err(Error::InvalidInput("Invalid core handle".into())),
                };
                let cancel = match h.get(&cancel) {
                    Some(Handle::Cancel(c)) => c.clone(),
                    _ => return Err(Error::InvalidInput("Invalid cancellation handle".into())),
                };
                (core, cancel)
            };
            let data = unsafe { std::slice::from_raw_parts(request, length) };
            let value: Value = serde_json::from_slice(data)
                .map_err(|e| Error::InvalidInput(format!("Invalid request JSON: {e}")))?;
            let events = Events {
                callback,
                context: context as usize,
            };
            if let Some(operation) = value
                .get("operation")
                .and_then(Value::as_str)
                .filter(|s| s.starts_with("transcription_"))
            {
                let operation = operation.to_owned();
                return transcription::execute(&operation, value, &cancel, &events);
            }
            let req = serde_json::from_value(value)
                .map_err(|e| Error::InvalidInput(format!("Invalid request JSON: {e}")))?;
            execute(&core, req, &cancel, &events)
        })();
        let envelope = match result {
            Ok(result) => json!({"schema_version":ABI_VERSION,"ok":true,"result":result}),
            Err(error) => {
                json!({"schema_version":ABI_VERSION,"ok":false,"error":error_json(&error)})
            }
        };
        insert(Handle::Bytes(Arc::new(envelope.to_string().into_bytes())))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_errors_retain_domains_and_fallback_causes() {
        let native = Error::NativeLibrary {
            library: "avid_whisper_bridge",
            operation: "load model",
            code: -10000,
            kind: "allocation",
            detail: "C++ allocation failed".into(),
        };
        let error = Error::Fallback {
            hardware: Box::new(Error::Linked {
                operation: "open encoder",
                code: -123,
                detail: "unavailable".into(),
            }),
            software: Box::new(native),
        };
        let value = error_json(&error);
        assert_eq!(value["hardware"]["library"], "libav");
        assert_eq!(value["hardware"]["native_code"], -123);
        assert_eq!(value["software"]["code"], "native_library_failed");
        assert_eq!(value["software"]["library"], "avid_whisper_bridge");
        assert_eq!(value["software"]["operation"], "load model");
        assert_eq!(value["software"]["native_code"], -10000);
        assert_eq!(value["software"]["native_kind"], "allocation");
    }
    #[test]
    fn invalid_handles_and_foreign_json_are_errors_without_unwinding() {
        let core = avid_core_create();
        let cancel = avid_core_cancel_create();
        for body in [
            b"null".as_slice(),
            b"{\"schema_version\":99,\"operation\":\"capabilities\"}",
        ] {
            let id = unsafe {
                avid_core_execute_json(
                    core,
                    cancel,
                    body.as_ptr(),
                    body.len(),
                    None,
                    std::ptr::null_mut(),
                )
            };
            let mut b = vec![0; avid_core_buffer_len(id)];
            assert_eq!(
                unsafe { avid_core_buffer_copy(id, b.as_mut_ptr(), b.len()) },
                b.len()
            );
            assert_eq!(serde_json::from_slice::<Value>(&b).unwrap()["ok"], false);
            assert_eq!(avid_core_release(id), 0);
            assert_eq!(avid_core_release(id), -1);
        }
        avid_core_release(core);
        avid_core_release(cancel);
        assert_eq!(avid_core_cancel(cancel), -1);
    }
}
