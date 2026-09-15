//! Opt-in managed runtime contract. Existing discovery remains compatible during migration.
use crate::{
    process::{run, RunOptions},
    CancellationToken, Error, MediaTools, Result, ToolDiscovery,
};
use serde_json::Value;
use std::{collections::HashSet, path::Path, time::Duration};

/// The authoritative build and compatibility specification embedded in this Core revision.
/// Hosts use this mapping to acquire artifacts; they never choose an FFmpeg version.
pub const FFMPEG_RUNTIME_SPECIFICATION: &str = include_str!("../runtime/ffmpeg/spec.json");

fn invalid(detail: impl Into<String>) -> Error {
    Error::ToolUnavailable {
        tool: "managed FFmpeg runtime",
        detail: detail.into(),
    }
}
fn specification() -> Value {
    serde_json::from_str(FFMPEG_RUNTIME_SPECIFICATION)
        .expect("checked-in FFmpeg specification is valid JSON")
}
/// Package basename, without archive extension, for a supported manifest target.
pub fn managed_runtime_artifact_name(target: &str) -> Result<String> {
    let spec = specification();
    if !spec["targets"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["id"] == target)
    {
        return Err(invalid(format!("Unsupported runtime target: {target}")));
    }
    Ok(format!(
        "avid-ffmpeg-{}-r{}-{target}",
        spec["source"]["version"].as_str().unwrap(),
        spec["recipe"]
    ))
}
fn native_target() -> String {
    let os = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(windows) {
        "windows"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        std::env::consts::ARCH
    };
    format!("{os}-{arch}")
}
fn read_json(path: &Path) -> Result<Value> {
    let bytes =
        std::fs::read(path).map_err(|e| Error::io("read managed runtime manifest", path, e))?;
    serde_json::from_slice(&bytes).map_err(|e| invalid(format!("Invalid runtime manifest: {e}")))
}
fn capability_names(text: &str) -> HashSet<&str> {
    let mut names = HashSet::new();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some(flags) = fields.next() else { continue };
        if let Some(name) = fields.next() {
            if flags.len() <= 6 && flags.chars().all(|c| c == '.' || c.is_ascii_uppercase()) {
                names.extend(name.split(','));
            }
        } else if flags
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            names.insert(flags);
        }
    }
    names
}
impl MediaTools {
    /// Resolve exactly one AVID-managed pair, with no bundle or PATH fallback.
    ///
    /// The host must first authenticate the downloaded package/checksums, and keep
    /// spec.json and build.json beside the tools when signing/repackaging them.
    /// This checks compatibility, not cryptographic authenticity. Candidate status
    /// is for testing: hosts must follow the publication/qualification gate before shipping.
    pub fn from_managed_directory(directory: &Path, token: &CancellationToken) -> Result<Self> {
        token.check()?;
        let spec = specification();
        if read_json(&directory.join("spec.json"))? != spec {
            return Err(invalid(
                "Runtime specification differs from this AVID Core revision",
            ));
        }
        let target_id = native_target();
        let target = spec["targets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["id"] == target_id)
            .ok_or_else(|| invalid("Unsupported native target"))?;
        let build = read_json(&directory.join("build.json"))?;
        if build["target"] != target_id
            || build["source_revision"] != spec["source"]["revision"]
            || build["recipe"] != spec["recipe"]
        {
            return Err(invalid(
                "Runtime build identity differs from the expected source, recipe or target",
            ));
        }
        let suffix = if cfg!(windows) { ".exe" } else { "" };
        let tools = Self::discover(
            ToolDiscovery {
                ffmpeg: Some(directory.join(format!("ffmpeg{suffix}"))),
                ffprobe: Some(directory.join(format!("ffprobe{suffix}"))),
                search_path: false,
                ..Default::default()
            },
            token,
        )?;
        for version in [tools.ffmpeg_version(), tools.ffprobe_version()] {
            if version.split_whitespace().nth(2) != spec["source"]["version"].as_str() {
                return Err(invalid(
                    "Runtime is not the exact expected stable FFmpeg release",
                ));
            }
        }
        for (category, required) in spec["required"].as_object().unwrap() {
            if category == "parsers" {
                continue;
            } // Build config + CI decode tests; no CLI listing exists.
            let output = run(
                tools.ffmpeg(),
                &["-hide_banner".into(), format!("-{category}").into()],
                token,
                RunOptions {
                    operation: "validate managed runtime capabilities",
                    timeout: Some(Duration::from_secs(30)),
                    progress_duration: None,
                    parse_progress: false,
                    events: &(),
                },
            )?;
            let text = String::from_utf8_lossy(&output);
            let names = capability_names(&text);
            let mut required = required.as_array().unwrap().clone();
            if category == "encoders" {
                required.extend(
                    target["required_encoders"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .cloned(),
                );
            }
            for value in required {
                let name = value.as_str().unwrap();
                if !names.contains(name) {
                    return Err(invalid(format!("Missing required {category}: {name}")));
                }
            }
        }
        Ok(tools)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_manifest_target_has_one_unambiguous_artifact() {
        let s = specification();
        let names: HashSet<_> = s["targets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| managed_runtime_artifact_name(t["id"].as_str().unwrap()).unwrap())
            .collect();
        assert_eq!(names.len(), 6);
        assert!(managed_runtime_artifact_name("unknown").is_err());
        assert!(!s["configure"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "--enable-nonfree"));
    }
    #[test]
    fn table_parser_does_not_match_descriptions_or_partial_names() {
        let n = capability_names(" V....D libx264 H264\n A..... fake description libx265\n DE mov,mp4 Movie\n  file\n T.. scale Scale video\n");
        for required in ["libx264", "mov", "mp4", "file", "scale"] {
            assert!(n.contains(required));
        }
        assert!(!n.contains("libx265"));
    }
    #[test]
    fn missing_managed_directory_never_uses_path() {
        let d = tempfile::tempdir().unwrap();
        assert!(
            MediaTools::from_managed_directory(d.path(), &CancellationToken::default()).is_err()
        );
    }
}
