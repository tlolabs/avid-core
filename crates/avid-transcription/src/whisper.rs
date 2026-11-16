use crate::{Error, ModelLease, Provenance, Result, Segment, TimingMethod, Transcript, Word};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct InferenceProgress {
    pub stage: &'static str,
    pub fraction: Option<f64>,
}
#[derive(Debug, Clone)]
pub struct WhisperCapabilities {
    pub runtime: &'static str,
    pub recognition: bool,
    /// Experimental recognizer BPE timestamps aggregated only at actual word
    /// boundaries. Missing native timestamps remain missing; never interpolated.
    pub recognizer_word_timing: bool,
    pub forced_alignment: bool,
    pub pcm_sample_rate: u32,
    pub supported_models: &'static [&'static str],
    pub recognition_languages: &'static [&'static str],
}
pub fn whisper_capabilities() -> WhisperCapabilities {
    WhisperCapabilities {
        runtime: "whisper.cpp-1.8.2",
        recognition: true,
        recognizer_word_timing: true,
        forced_alignment: false,
        pcm_sample_rate: 16_000,
        supported_models: &["whisper-cpp-tiny-en"],
        recognition_languages: &["en"],
    }
}

fn native_inference_error(error: avid_whisper_bridge::Error) -> Error {
    if error.kind == avid_whisper_bridge::ErrorKind::Cancelled {
        Error::Cancelled
    } else {
        Error::NativeInference {
            code: error.code,
            kind: error.kind.as_str(),
            operation: error.operation,
            detail: error.detail,
        }
    }
}

fn native_media_error(error: avid_media_bridge::Error, cancelled: bool) -> Error {
    if cancelled {
        Error::Cancelled
    } else {
        Error::NativeMedia {
            code: error.code,
            operation: error.operation,
            detail: error.detail,
        }
    }
}
pub struct RecognitionOptions<'a> {
    pub language: Option<&'a str>,
    pub source_offset: f64,
    pub threads: u32,
}
impl Default for RecognitionOptions<'_> {
    fn default() -> Self {
        Self {
            language: None,
            source_offset: 0.,
            threads: 4,
        }
    }
}

fn grouped_words(tokens: Vec<avid_whisper_bridge::Token>, offset: f64) -> Result<Vec<Word>> {
    let mut groups: Vec<Vec<avid_whisper_bridge::Token>> = Vec::new();
    for token in tokens {
        if groups.is_empty() || token.bytes.first().is_some_and(|v| v.is_ascii_whitespace()) {
            groups.push(Vec::new());
        }
        groups.last_mut().unwrap().push(token);
    }
    groups
        .into_iter()
        .filter_map(|group| {
            let bytes: Vec<_> = group.iter().flat_map(|v| v.bytes.iter().copied()).collect();
            let text = match String::from_utf8(bytes) {
                Ok(v) => v.trim().to_string(),
                Err(_) => {
                    return Some(Err(Error::Inference(
                        "recognizer returned invalid UTF-8 word bytes".into(),
                    )))
                }
            };
            if text.is_empty() {
                return None;
            }
            let complete = group.iter().all(|v| {
                v.start_seconds.is_finite()
                    && v.end_seconds.is_finite()
                    && v.start_seconds >= 0.
                    && v.end_seconds >= v.start_seconds
            });
            let start = complete.then(|| {
                group
                    .iter()
                    .map(|v| v.start_seconds)
                    .fold(f64::INFINITY, f64::min)
                    + offset
            });
            let end =
                complete.then(|| group.iter().map(|v| v.end_seconds).fold(0., f64::max) + offset);
            Some(Ok(Word {
                text,
                start_seconds: start,
                end_seconds: end,
                confidence: None,
                timing_method: complete.then_some(TimingMethod::Recognizer),
            }))
        })
        .collect()
}
/// Blocking, offline, directly linked inference. Call on a host worker. The
/// verified lease must outlive this call; model context unloads before return.
/// PCM memory is bounded to one 30-second window plus one decoded frame.
pub fn recognize_audio(
    lease: &ModelLease,
    audio: &Path,
    options: &RecognitionOptions<'_>,
    cancelled: &(dyn Fn() -> bool + Sync),
    progress: &dyn Fn(InferenceProgress),
) -> Result<Transcript> {
    if !options.source_offset.is_finite()
        || options.source_offset < 0.
        || !(1..=64).contains(&options.threads)
    {
        return Err(Error::InvalidInput(
            "invalid source offset/thread count".into(),
        ));
    }
    if cancelled() {
        return Err(Error::Cancelled);
    }
    if lease.manifest() != &crate::ModelManifest::tiny_english() {
        return Err(Error::Inference("this adapter only supports the pinned whisper-cpp-tiny-en manifest; custom conversions require explicit qualification".into()));
    }
    let language = options
        .language
        .map(|v| {
            v.trim()
                .split(['-', '_'])
                .next()
                .unwrap_or("")
                .to_ascii_lowercase()
        })
        .or_else(|| Some("en".into()));
    if language.as_deref() != Some("en") {
        return Err(Error::InvalidInput("whisper-cpp-tiny-en only supports English; select a different explicitly implemented model/provider".into()));
    }
    progress(InferenceProgress {
        stage: "loading",
        fraction: Some(0.),
    });
    let control = avid_whisper_bridge::Control { cancelled };
    let mut model =
        avid_whisper_bridge::Model::open(lease.path(), &control).map_err(native_inference_error)?;
    let no_progress = |_: f64| {};
    let media_control = avid_media_bridge::Control {
        cancelled,
        progress: &no_progress,
    };
    let metadata = avid_media_bridge::probe(audio, &media_control)
        .map_err(|error| native_media_error(error, cancelled()))?;
    if !metadata.has_audio {
        return Err(Error::InvalidInput("input contains no audio track".into()));
    }
    let mut pending = Vec::<f32>::with_capacity(16_000 * 30);
    let mut consumed = 0usize;
    let mut segments = Vec::new();
    let mut infer = |pcm: &[f32], consumed: usize| -> Result<()> {
        if cancelled() {
            return Err(Error::Cancelled);
        }
        let offset = options.source_offset + consumed as f64 / 16_000.;
        let result = model
            .recognize(pcm, language.as_deref(), options.threads, &control)
            .map_err(native_inference_error)?;
        for segment in result {
            if segment.text.trim().is_empty() {
                continue;
            }
            segments.push(Segment {
                text: segment.text.trim().into(),
                words: grouped_words(segment.tokens, offset)?,
                start_seconds: Some(segment.start_seconds + offset),
                end_seconds: Some(segment.end_seconds + offset),
            });
        }
        progress(InferenceProgress {
            stage: "recognizing",
            fraction: metadata
                .duration
                .map(|d| ((consumed + pcm.len()) as f64 / 16_000. / d).min(1.)),
        });
        Ok(())
    };
    let mut inference_error = None;
    let decoded = avid_media_bridge::decode_audio(audio, 16_000, 1, &media_control, &mut |chunk| {
        let expected = (consumed + pending.len()) as f64 / 16_000.;
        if (chunk.start_seconds - expected).abs() > 0.001 {
            inference_error = Some(Error::InvalidInput(
                "decoded audio clock is discontinuous".into(),
            ));
            return false;
        }
        for part in chunk.samples.chunks(16_000) {
            let mut rest = part;
            while !rest.is_empty() {
                let count = rest.len().min(16_000 * 30 - pending.len());
                pending.extend_from_slice(&rest[..count]);
                rest = &rest[count..];
                if pending.len() == 16_000 * 30 {
                    if let Err(error) = infer(&pending, consumed) {
                        inference_error = Some(error);
                        return false;
                    }
                    consumed += pending.len();
                    pending.clear();
                }
            }
        }
        true
    });
    if let Some(error) = inference_error {
        return Err(error);
    }
    decoded.map_err(|error| native_media_error(error, cancelled()))?;
    if !pending.is_empty() {
        infer(&pending, consumed)?;
    }
    drop(infer);
    drop(model);
    if cancelled() {
        return Err(Error::Cancelled);
    }
    let transcript = Transcript {
        segments,
        language,
        provenance: Provenance {
            engine_id: "whisper.cpp-1.8.2".into(),
            model_id: lease.manifest().id.clone(),
            model_identity: Some(lease.manifest().identity()?),
            runtime_revisions: Some(std::collections::BTreeMap::from([(
                "whisper.cpp".into(),
                crate::WHISPER_REVISION.into(),
            )])),
            runtime_build_sha256: Some(avid_whisper_bridge::BUILD_IDENTITY_SHA256.into()),
        },
        alignment_provenance: None,
    };
    transcript.validate()?;
    progress(InferenceProgress {
        stage: "complete",
        fraction: Some(1.),
    });
    Ok(transcript)
}
/// A forced-alignment request cannot be emulated with recognition or prompts.
pub fn align_corrected_text(
    _lease: &ModelLease,
    _audio: &Path,
    _transcript: &Transcript,
) -> Result<Transcript> {
    Err(Error::Inference("whisper.cpp adapter has no forced-alignment capability; select an explicitly supported aligner".into()))
}
