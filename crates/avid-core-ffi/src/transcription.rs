//! Safe request routing for transcript contracts and optional linked inference.
#[cfg(feature = "whisper")]
use avid_core::Progress;
use avid_core::{CancellationToken, Error, EventSink};
use avid_transcription::{
    caption_cues, export_captions, CaptionOptions, CaptionWindow, ModelManifest, ModelStore,
    Transcript,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;

fn invalid(value: impl ToString) -> Error {
    Error::InvalidInput(value.to_string())
}
fn map_error(error: avid_transcription::Error) -> Error {
    match error {
        avid_transcription::Error::Cancelled => Error::Cancelled,
        avid_transcription::Error::NativeMedia {
            code,
            operation,
            detail,
        } => Error::Linked {
            operation: "decode transcription audio",
            code,
            detail: format!("{operation}: {detail}"),
        },
        avid_transcription::Error::NativeInference {
            code,
            kind,
            operation,
            detail,
        } => Error::NativeLibrary {
            library: "avid_whisper_bridge",
            operation,
            code,
            kind,
            detail,
        },
        // Contract validation has no native status; never fabricate a libav code.
        avid_transcription::Error::Inference(detail) => invalid(detail),
        other => invalid(other),
    }
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    schema_version: u32,
    operation: String,
    transcript: Option<Transcript>,
    #[serde(default)]
    corrections: Vec<Correction>,
    #[serde(default)]
    options: CaptionSettings,
    #[serde(default)]
    window: Window,
    cache: Option<String>,
    model: Option<ModelManifest>,
    source: Option<String>,
    audio: Option<String>,
    language: Option<String>,
    #[serde(default)]
    source_offset: f64,
    #[serde(default = "threads")]
    threads: u32,
}
fn threads() -> u32 {
    4
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Correction {
    index: usize,
    text: String,
}
#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct CaptionSettings {
    maximum_characters: usize,
    maximum_words: usize,
    maximum_duration: f64,
    maximum_pause: f64,
}
impl Default for CaptionSettings {
    fn default() -> Self {
        let v = CaptionOptions::default();
        Self {
            maximum_characters: v.maximum_characters,
            maximum_words: v.maximum_words,
            maximum_duration: v.maximum_duration,
            maximum_pause: v.maximum_pause,
        }
    }
}
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Window {
    source_start: f64,
    source_end: Option<f64>,
    timeline_start: f64,
}
fn local(value: Option<String>, name: &str) -> std::result::Result<PathBuf, Error> {
    let value = value.ok_or_else(|| invalid(format!("missing {name}")))?;
    if value.is_empty()
        || value.contains('\0')
        || value.contains("://")
        || value.starts_with("content:")
        || value.starts_with("file:")
    {
        return Err(invalid(format!(
            "{name} must be an accessible local filesystem path; import mobile document URLs first"
        )));
    }
    Ok(value.into())
}
pub fn execute(
    operation: &str,
    value: Value,
    cancel: &CancellationToken,
    events: &dyn EventSink,
) -> std::result::Result<Value, Error> {
    let request: Request = serde_json::from_value(value).map_err(invalid)?;
    if request.schema_version != 1 || request.operation != operation {
        return Err(invalid(
            "unsupported transcription request schema/operation",
        ));
    }
    if cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    match operation {
        "transcription_capabilities" => Ok(json!({"contracts_schema":1,"transcript_validation":true,"batch_corrections":true,"captions":["srt","webvtt"],"whisper_cpp":{"compiled":cfg!(feature="whisper"),"runtime":"whisper.cpp-1.8.2","model_format":"ggml","supported_models":["whisper-cpp-tiny-en"],"languages":["en"],"model_selection_required":true,"recognition":cfg!(feature="whisper"),"native_recognizer_token_timing":cfg!(feature="whisper"),"forced_alignment":false,"native_qualification":"see docs/linked-transcription.md"},"apple_providers":{"interface":"Swift packages AVIDTranscription/AVIDTranscriptionEngines","available_through_this_abi":false},"portable_qwen_alignment":false,"implicit_downloads":false})),
        "transcription_models" => Ok(json!({"models":[ModelManifest::tiny_english()],"default_model":null,"explicit_selection_required":true})),
        "transcription_validate" => { let transcript = request.transcript.ok_or_else(|| invalid("missing transcript"))?; transcript.validate().map_err(map_error)?; Ok(json!({"valid":true,"complete_word_timing":transcript.has_complete_word_timing()})) },
        "transcription_correct" => { let transcript = request.transcript.ok_or_else(|| invalid("missing transcript"))?; let corrections = request.corrections.into_iter().map(|v| (v.index, v.text)).collect::<Vec<_>>(); let corrected = transcript.correcting(&corrections).map_err(map_error)?; serde_json::to_value(corrected).map_err(invalid) },
        "transcription_srt" | "transcription_webvtt" => {
            let transcript = request.transcript.ok_or_else(|| invalid("missing transcript"))?;
            let o = request.options; let w = request.window;
            let cues = caption_cues(&transcript, &CaptionOptions { maximum_characters:o.maximum_characters, maximum_words:o.maximum_words, maximum_duration:o.maximum_duration, maximum_pause:o.maximum_pause }, &CaptionWindow { source_start:w.source_start, source_end:w.source_end, timeline_start:w.timeline_start }).map_err(map_error)?;
            let text = export_captions(&cues, operation == "transcription_webvtt").map_err(map_error)?;
            Ok(json!({"text":text,"cues":cues}))
        },
        "transcription_model_import" | "transcription_model_verify" | "transcription_model_remove" | "transcription_recognize" => {
            let manifest = request.model.ok_or_else(|| invalid("model manifest must be selected explicitly; query transcription_models"))?;
            manifest.validate().map_err(map_error)?;
            let store = ModelStore::new(local(request.cache, "cache")?).map_err(map_error)?;
            let cancelled = || cancel.is_cancelled();
            match operation {
                "transcription_model_import" => { store.import(&manifest, &local(request.source, "source")?, &cancelled).map_err(map_error)?; Ok(json!({"installed":manifest.id,"identity":manifest.installation_id().map_err(map_error)?})) },
                "transcription_model_verify" => { let lease = store.lease(&manifest,&cancelled).map_err(map_error)?; Ok(json!({"verified":lease.manifest().id,"identity":lease.manifest().installation_id().map_err(map_error)?})) },
                "transcription_model_remove" => { store.remove(&manifest).map_err(map_error)?; Ok(json!({"removed":manifest.id})) },
                _ => {
                    #[cfg(feature="whisper")]
                    {
                        let lease = store.lease(&manifest,&cancelled).map_err(map_error)?;
                        let audio = local(request.audio, "audio")?;
                        let options = avid_transcription::RecognitionOptions { language:request.language.as_deref(), source_offset:request.source_offset, threads:request.threads };
                        let transcript = avid_transcription::recognize_audio(&lease,&audio,&options,&cancelled,&|v| events.progress(Progress { fraction:v.fraction,..Default::default() })).map_err(map_error)?;
                        serde_json::to_value(transcript).map_err(invalid)
                    }
                    #[cfg(not(feature="whisper"))]
                    { let _ = (&events, &request.audio, &request.language, request.source_offset, request.threads); Err(Error::Unsupported { capability:"linked whisper recognition".into(),detail:"this artifact was built without the whisper feature; use the pinned linked-inference build profile".into() }) }
                }
            }
        },
        "transcription_align" => Err(Error::Unsupported { capability:"portable forced alignment".into(),detail:"no portable forced aligner is implemented; the preserved Apple QwenAlignmentProvider is available through Swift, with its own model and platform requirements".into() }),
        _ => Err(Error::Unsupported { capability:operation.into(),detail:"unknown transcription operation".into() }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn availability_and_missing_alignment_are_explicit() {
        let token = CancellationToken::default();
        let request = json!({"schema_version":1,"operation":"transcription_capabilities"});
        assert_eq!(
            execute("transcription_capabilities", request, &token, &()).unwrap()
                ["portable_qwen_alignment"],
            false
        );
        let request = json!({"schema_version":1,"operation":"transcription_align"});
        assert!(matches!(
            execute("transcription_align", request, &token, &()),
            Err(Error::Unsupported { .. })
        ));
    }
}
