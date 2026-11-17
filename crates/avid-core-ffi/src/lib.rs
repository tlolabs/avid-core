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
    if s.is_empty()
        || s.contains('\0')
        || s.contains("://")
        || s.split_once(':').is_some_and(|(scheme, _)| {
            scheme.eq_ignore_ascii_case("content") || scheme.eq_ignore_ascii_case("file")
        })
    {
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

    fn response(id: u64) -> Value {
        assert_ne!(id, 0, "ABI boundary failed");
        let mut bytes = vec![0; avid_core_buffer_len(id)];
        assert_eq!(
            unsafe { avid_core_buffer_copy(id, bytes.as_mut_ptr(), bytes.len()) },
            bytes.len()
        );
        assert_eq!(avid_core_release(id), 0);
        serde_json::from_slice(&bytes).unwrap()
    }

    fn call(core: u64, cancel: u64, body: &[u8]) -> Value {
        response(unsafe {
            avid_core_execute_json(
                core,
                cancel,
                body.as_ptr(),
                body.len(),
                None,
                std::ptr::null_mut(),
            )
        })
    }

    #[cfg(unix)]
    #[test]
    fn reserved_uri_prefixes_require_explicit_local_path_spelling() {
        for name in ["file:1.png", "FILE:1.png", "content:name", "CoNtEnT:name"] {
            assert!(
                matches!(
                    path(Some(name.into()), "image"),
                    Err(Error::InvalidInput(_))
                ),
                "ambiguous URI prefix must remain rejected: {name}"
            );
        }
        for name in ["./file:1.png", "./content:name"] {
            assert_eq!(
                path(Some(name.into()), "image").unwrap(),
                PathBuf::from(name)
            );
        }

        let directory = tempfile::tempdir().unwrap();
        let mut pixels = Vec::new();
        let mut encoder = png::Encoder::new(&mut pixels, 16, 16);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[255, 0, 0].repeat(16 * 16))
            .unwrap();
        let core = avid_core_create();
        let cancel = avid_core_cancel_create();
        for name in ["file:1.png", "content:name"] {
            let image = directory.path().join(name);
            std::fs::write(&image, &pixels).unwrap();
            assert_eq!(
                path(Some(image.to_str().unwrap().into()), "image").unwrap(),
                image
            );
            let body = serde_json::to_vec(&json!({
                "schema_version": 1, "operation": "inspect_image", "image": image
            }))
            .unwrap();
            let value = call(core, cancel, &body);
            assert_eq!(value["ok"], true, "{value}");
            assert_eq!(value["result"], json!({"width": 16, "height": 16}));
        }
        assert_eq!(avid_core_release(core), 0);
        assert_eq!(avid_core_release(cancel), 0);
    }

    #[test]
    fn foreign_spans_types_paths_and_short_buffers_are_validated() {
        let core = avid_core_create();
        let cancel = avid_core_cancel_create();
        for body in [
            b"".as_slice(),
            b"\xff",
            br#"{"schema_version":1,"operation":"capabilities","unexpected":true}"#,
            br#"{"schema_version":1,"operation":"inspect_image","image":"content:document/12"}"#,
            br#"{"schema_version":1,"operation":"inspect_image","image":"file:/import/art.png"}"#,
            br#"{"schema_version":1,"operation":"inspect_image","image":"FILE:/import/art.png"}"#,
            br#"{"schema_version":1,"operation":"inspect_image","image":"content://document/12"}"#,
            br#"{"schema_version":1,"operation":"inspect_image","image":"bad\u0000path"}"#,
        ] {
            let value = call(core, cancel, body);
            assert_eq!(value["ok"], false, "{value}");
            assert_eq!(value["error"]["code"], "invalid_input", "{value}");
        }
        for body in [
            vec![b' '; MAX_REQUEST + 1],
            br#"{"schema_version":1,"operation":"capabilities"}"#.to_vec(),
        ] {
            // Oversized spans and type-confused handles both return structured errors.
            let value = call(cancel, core, &body);
            assert_eq!(value["ok"], false);
            assert_eq!(value["error"]["code"], "invalid_input");
        }
        let value = response(unsafe {
            avid_core_execute_json(
                core,
                cancel,
                std::ptr::null(),
                10,
                None,
                std::ptr::null_mut(),
            )
        });
        assert_eq!(value["error"]["code"], "invalid_input");
        let result = unsafe {
            avid_core_execute_json(
                core,
                cancel,
                b"null".as_ptr(),
                4,
                None,
                std::ptr::null_mut(),
            )
        };
        let mut destination = [0xa5; 8];
        assert_eq!(
            unsafe { avid_core_buffer_copy(result, destination.as_mut_ptr(), destination.len()) },
            0
        );
        assert_eq!(destination, [0xa5; 8]);
        assert_eq!(avid_core_release(result), 0);
        assert_eq!(avid_core_release(core), 0);
        assert_eq!(avid_core_release(cancel), 0);
    }

    struct ReentrantEvents {
        core: u64,
        cancel: u64,
        caller: std::thread::ThreadId,
        callbacks: usize,
        wrong_thread: bool,
        released: bool,
        nested_ok: bool,
    }
    unsafe extern "C" fn reentrant_event(context: *mut c_void, _: *const u8, _: usize) {
        let events = unsafe { &mut *context.cast::<ReentrantEvents>() };
        events.callbacks += 1;
        events.wrong_thread |= std::thread::current().id() != events.caller;
        if events.callbacks != 1 {
            return;
        }
        // No registry lock is held across foreign callbacks. An independent
        // operation and handle release are legal while the outer call retains
        // its own Core and cancellation references.
        let body = br#"{"schema_version":1,"operation":"unsupported_reentrant_probe"}"#;
        let nested = unsafe {
            avid_core_execute_json(
                events.core,
                events.cancel,
                body.as_ptr(),
                body.len(),
                None,
                std::ptr::null_mut(),
            )
        };
        let size = avid_core_buffer_len(nested);
        let mut bytes = vec![0; size];
        if size > 0 && unsafe { avid_core_buffer_copy(nested, bytes.as_mut_ptr(), size) } == size {
            events.nested_ok = serde_json::from_slice::<Value>(&bytes)
                .is_ok_and(|v| v["error"]["code"] == "unsupported_capability");
        }
        avid_core_release(nested);
        events.released =
            avid_core_release(events.core) == 0 && avid_core_release(events.cancel) == 0;
    }

    #[test]
    fn callback_reentrancy_and_handle_release_preserve_active_job_lifetime() {
        let directory = tempfile::tempdir().unwrap();
        let image = directory.path().join("artwork ü.png");
        let output = directory.path().join("preview ü.png");
        let mut encoder = png::Encoder::new(std::fs::File::create(&image).unwrap(), 16, 16);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[255, 0, 0].repeat(16 * 16))
            .unwrap();
        let mut events = ReentrantEvents {
            core: avid_core_create(),
            cancel: avid_core_cancel_create(),
            caller: std::thread::current().id(),
            callbacks: 0,
            wrong_thread: false,
            released: false,
            nested_ok: false,
        };
        let body = serde_json::to_vec(&json!({
            "schema_version": 1, "operation": "preview", "image": image,
            "output": output, "settings": {"width":160,"height":90}
        }))
        .unwrap();
        let value = response(unsafe {
            avid_core_execute_json(
                events.core,
                events.cancel,
                body.as_ptr(),
                body.len(),
                Some(reentrant_event),
                std::ptr::from_mut(&mut events).cast(),
            )
        });
        assert_eq!(value["ok"], true, "{value}");
        assert!(events.callbacks >= 4);
        assert!(!events.wrong_thread);
        assert!(events.released && events.nested_ok);
        assert_eq!(avid_core_release(events.core), -1);
        assert_eq!(avid_core_cancel(events.cancel), -1);
        assert!(std::fs::metadata(output).unwrap().len() > 0);
    }
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
