use crate::{Error, ModelIdentity, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provenance {
    #[serde(rename = "engineID")]
    pub engine_id: String,
    #[serde(rename = "modelID")]
    pub model_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_identity: Option<ModelIdentity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_revisions: Option<BTreeMap<String, String>>,
    #[serde(
        rename = "runtimeBuildSHA256",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub runtime_build_sha256: Option<String>,
}
impl Provenance {
    /// Legacy or unpinned providers keep their identity unknown rather than
    /// inferring a current catalog revision for historical transcripts.
    pub fn new(engine_id: impl Into<String>, model_id: impl Into<String>) -> Self {
        Self {
            engine_id: engine_id.into(),
            model_id: model_id.into(),
            model_identity: None,
            runtime_revisions: None,
            runtime_build_sha256: None,
        }
    }
    pub fn validate(&self) -> Result<()> {
        if self.engine_id.trim().is_empty()
            || self.model_id.trim().is_empty()
            || self.engine_id.bytes().any(|v| v.is_ascii_control())
            || self.model_id.bytes().any(|v| v.is_ascii_control())
        {
            return Err(Error::InvalidInput(
                "transcript provenance is required".into(),
            ));
        }
        if let Some(identity) = &self.model_identity {
            identity.validate()?;
        }
        if let Some(revisions) = &self.runtime_revisions {
            if revisions.is_empty()
                || revisions.iter().any(|(runtime, revision)| {
                    !crate::model::identity_component(runtime)
                        || !crate::model::pinned_hex(revision, 40)
                })
            {
                return Err(Error::InvalidInput(
                    "invalid pinned runtime revisions".into(),
                ));
            }
        }
        if self
            .runtime_build_sha256
            .as_ref()
            .is_some_and(|v| !crate::model::pinned_hex(v, 64))
        {
            return Err(Error::InvalidInput(
                "invalid pinned runtime build SHA-256".into(),
            ));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TimingMethod {
    Recognizer,
    ForcedAlignment,
    Manual,
    Estimated,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Word {
    pub text: String,
    pub start_seconds: Option<f64>,
    pub end_seconds: Option<f64>,
    pub confidence: Option<f64>,
    pub timing_method: Option<TimingMethod>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub text: String,
    #[serde(default)]
    pub words: Vec<Word>,
    pub start_seconds: Option<f64>,
    pub end_seconds: Option<f64>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transcript {
    pub segments: Vec<Segment>,
    pub language: Option<String>,
    pub provenance: Provenance,
    pub alignment_provenance: Option<Provenance>,
}
fn times(start: Option<f64>, end: Option<f64>) -> Result<()> {
    match (start, end) {
        (None, None) => Ok(()),
        (Some(a), Some(b)) if a.is_finite() && b.is_finite() && a >= 0. && b >= a => Ok(()),
        _ => Err(Error::InvalidInput(
            "times must be paired, finite, nonnegative and ordered".into(),
        )),
    }
}
impl Transcript {
    /// Validate after deserialization and before every operation. Public mutable
    /// records are deliberately plain data; invalid data never reaches inference/export.
    pub fn validate(&self) -> Result<()> {
        self.provenance.validate()?;
        if let Some(provenance) = &self.alignment_provenance {
            provenance.validate()?;
        }
        let mut previous_start = 0.;
        for segment in &self.segments {
            times(segment.start_seconds, segment.end_seconds)?;
            for word in &segment.words {
                times(word.start_seconds, word.end_seconds)?;
                if let Some(c) = word.confidence {
                    if !c.is_finite() || !(0. ..=1.).contains(&c) {
                        return Err(Error::InvalidInput(
                            "confidence must be between zero and one".into(),
                        ));
                    }
                }
                if word.start_seconds.is_none() && word.timing_method.is_some() {
                    return Err(Error::InvalidInput(
                        "untimed words cannot declare timing provenance".into(),
                    ));
                }
                if let Some(start) = word.start_seconds {
                    if start < previous_start {
                        return Err(Error::InvalidInput(
                            "word times must be ordered on the source clock".into(),
                        ));
                    }
                    previous_start = start;
                }
            }
        }
        Ok(())
    }
    pub fn from_json(bytes: &[u8]) -> Result<Self> {
        let transcript: Self =
            serde_json::from_slice(bytes).map_err(|v| Error::InvalidInput(v.to_string()))?;
        transcript.validate()?;
        Ok(transcript)
    }
    pub fn has_complete_word_timing(&self) -> bool {
        self.segments.iter().all(|s| {
            s.text.trim().is_empty()
                || (!s.words.is_empty()
                    && s.words
                        .iter()
                        .all(|w| w.start_seconds.is_some() && w.end_seconds.is_some()))
        })
    }
    /// Atomic batch correction: all indices are checked before any change. Only
    /// edited segments lose their words; the transcript-level alignment claim is cleared.
    pub fn correcting(&self, changes: &[(usize, String)]) -> Result<Self> {
        self.validate()?;
        let mut indices = std::collections::HashSet::new();
        for (index, _) in changes {
            if *index >= self.segments.len() || !indices.insert(*index) {
                return Err(Error::InvalidInput(
                    "invalid or repeated correction index".into(),
                ));
            }
        }
        let mut result = self.clone();
        for (index, text) in changes {
            result.segments[*index].text.clone_from(text);
            result.segments[*index].words.clear();
        }
        if !changes.is_empty() {
            result.alignment_provenance = None;
        }
        Ok(result)
    }
    pub fn offsetting(&self, offset: f64) -> Result<Self> {
        if !offset.is_finite() {
            return Err(Error::InvalidInput("source offset must be finite".into()));
        }
        let mut result = self.clone();
        for segment in &mut result.segments {
            segment.start_seconds = segment.start_seconds.map(|v| v + offset);
            segment.end_seconds = segment.end_seconds.map(|v| v + offset);
            for word in &mut segment.words {
                word.start_seconds = word.start_seconds.map(|v| v + offset);
                word.end_seconds = word.end_seconds.map(|v| v + offset);
            }
        }
        result.validate()?;
        Ok(result)
    }
}
#[derive(Debug, Clone)]
pub struct CaptionOptions {
    pub maximum_characters: usize,
    pub maximum_words: usize,
    pub maximum_duration: f64,
    pub maximum_pause: f64,
}
impl Default for CaptionOptions {
    fn default() -> Self {
        Self {
            maximum_characters: 42,
            maximum_words: 6,
            maximum_duration: 3.,
            maximum_pause: 0.6,
        }
    }
}
#[derive(Debug, Clone, Default)]
pub struct CaptionWindow {
    pub source_start: f64,
    pub source_end: Option<f64>,
    pub timeline_start: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CaptionCue {
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub text: String,
    pub words: Vec<Word>,
}
fn joined<'a>(words: impl IntoIterator<Item = &'a Word>, language: Option<&str>) -> String {
    let unspaced = matches!(
        language.unwrap_or("").split(['-', '_']).next(),
        Some("zh" | "ja" | "th" | "lo" | "km" | "my")
    );
    let mut result = String::new();
    for word in words {
        let cjk = |c: char| matches!(c as u32, 0x3040..=0x30ff|0x3400..=0x9fff|0xf900..=0xfaff|0x20000..=0x3134f);
        let start = word.text.chars().next();
        let end = result.chars().last();
        if !result.is_empty()
            && !unspaced
            && !start.is_some_and(|v| {
                v.is_whitespace() || ".,!?;:%…)]}’”。，、！？；：％）》」』】".contains(v) || cjk(v)
            })
            && !end.is_some_and(|v| v.is_whitespace() || "([{‘“（《「『【".contains(v) || cjk(v))
        {
            result.push(' ');
        }
        result.push_str(&word.text);
    }
    result.trim().into()
}
pub fn caption_cues(
    transcript: &Transcript,
    options: &CaptionOptions,
    window: &CaptionWindow,
) -> Result<Vec<CaptionCue>> {
    transcript.validate()?;
    if !transcript.has_complete_word_timing() {
        return Err(Error::MissingWordTiming);
    }
    if options.maximum_characters == 0
        || options.maximum_words == 0
        || !options.maximum_duration.is_finite()
        || options.maximum_duration <= 0.
        || !options.maximum_pause.is_finite()
        || options.maximum_pause < 0.
        || !window.timeline_start.is_finite()
        || window.timeline_start < 0.
    {
        return Err(Error::InvalidInput("invalid caption limits/window".into()));
    }
    times(
        Some(window.source_start),
        Some(window.source_end.unwrap_or(window.source_start)),
    )?;
    let mut cues = Vec::new();
    let mut group = Vec::<Word>::new();
    let flush = |group: &mut Vec<Word>, cues: &mut Vec<CaptionCue>| {
        if let Some(start) = group.first().and_then(|v| v.start_seconds) {
            let end = group
                .iter()
                .filter_map(|v| v.end_seconds)
                .fold(start, f64::max);
            if end > start {
                cues.push(CaptionCue {
                    start_seconds: start,
                    end_seconds: end,
                    text: joined(group.iter(), transcript.language.as_deref()),
                    words: std::mem::take(group),
                });
            }
        }
        group.clear();
    };
    for word in transcript.segments.iter().flat_map(|v| &v.words) {
        let (Some(start), Some(end)) = (word.start_seconds, word.end_seconds) else {
            return Err(Error::MissingWordTiming);
        };
        let trim_end = window.source_end.unwrap_or(f64::INFINITY);
        if word.text.trim().is_empty()
            || end < window.source_start
            || start >= trim_end
            || (end == window.source_start && start < end)
        {
            continue;
        }
        let start = start.max(window.source_start) - window.source_start + window.timeline_start;
        let end = end.min(trim_end) - window.source_start + window.timeline_start;
        times(Some(start), Some(end))?;
        let mut word = word.clone();
        word.start_seconds = Some(start);
        word.end_seconds = Some(end);
        if let (Some(first), Some(last)) = (group.first(), group.last()) {
            if group.len() >= options.maximum_words
                || end - first.start_seconds.unwrap() > options.maximum_duration
                || start - last.end_seconds.unwrap() > options.maximum_pause
                || joined(
                    group.iter().chain(std::iter::once(&word)),
                    transcript.language.as_deref(),
                )
                .chars()
                .count()
                    > options.maximum_characters
            {
                flush(&mut group, &mut cues);
            }
        }
        let sentence = word
            .text
            .trim()
            .ends_with(['.', '!', '?', '。', '！', '？', '…']);
        group.push(word);
        if sentence {
            flush(&mut group, &mut cues);
        }
    }
    flush(&mut group, &mut cues);
    Ok(cues)
}
pub fn export_captions(cues: &[CaptionCue], web_vtt: bool) -> Result<String> {
    fn timestamp(value: f64, separator: char) -> Result<String> {
        if !value.is_finite() || !(0. ..=9_000_000_000_000.).contains(&value) {
            return Err(Error::InvalidInput(
                "caption timestamp exceeds export limits".into(),
            ));
        }
        let ms = (value * 1000.).round() as u64;
        Ok(format!(
            "{:02}:{:02}:{:02}{separator}{:03}",
            ms / 3_600_000,
            ms / 60_000 % 60,
            ms / 1000 % 60,
            ms % 1000
        ))
    }
    let mut result = if web_vtt {
        "WEBVTT\n\n".into()
    } else {
        String::new()
    };
    let mut previous = 0.;
    for (index, cue) in cues.iter().enumerate() {
        times(Some(cue.start_seconds), Some(cue.end_seconds))?;
        if cue.end_seconds <= cue.start_seconds || cue.start_seconds < previous {
            return Err(Error::InvalidInput(
                "caption cues must have ordered positive durations".into(),
            ));
        }
        previous = cue.start_seconds;
        if !web_vtt {
            result.push_str(&format!("{}\n", index + 1));
        }
        let separator = if web_vtt { '.' } else { ',' };
        let end = cue
            .end_seconds
            .max(((cue.start_seconds * 1000.).round() + 1.) / 1000.);
        result.push_str(&format!(
            "{} --> {}\n",
            timestamp(cue.start_seconds, separator)?,
            timestamp(end, separator)?
        ));
        let text = cue
            .text
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        result.push_str(&text.lines().collect::<Vec<_>>().join(" "));
        result.push_str("\n\n");
    }
    Ok(result)
}
