use crate::{
    process::{run, RunOptions},
    CancellationToken, Error, Result,
};
use std::{
    env,
    path::{Path, PathBuf},
    time::Duration,
};

/// Host environment overrides are resolved by the host, then passed explicitly.
#[derive(Clone, Debug)]
pub struct ToolDiscovery {
    pub ffmpeg: Option<PathBuf>,
    pub ffprobe: Option<PathBuf>,
    /// Searched in order before the standard adjacent/bundle directories.
    pub directories: Vec<PathBuf>,
    pub search_path: bool,
    pub validation_timeout: Duration,
}
impl Default for ToolDiscovery {
    fn default() -> Self {
        Self {
            ffmpeg: None,
            ffprobe: None,
            directories: Vec::new(),
            search_path: true,
            validation_timeout: Duration::from_secs(30),
        }
    }
}
#[derive(Clone, Debug)]
pub struct MediaTools {
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
    ffmpeg_version: String,
    ffprobe_version: String,
}
impl MediaTools {
    pub fn discover(config: ToolDiscovery, token: &CancellationToken) -> Result<Self> {
        token.check()?;
        let mut directories = config.directories;
        if let Ok(executable) = env::current_exe() {
            if let Some(directory) = executable.parent() {
                directories.push(directory.to_path_buf());
                directories.push(directory.join("ffmpeg"));
                if let Some(contents) = directory.parent() {
                    directories.push(contents.join("Resources"));
                    directories.push(contents.join("Frameworks"));
                }
            }
        }
        if config.search_path {
            if let Some(paths) = env::var_os("PATH") {
                directories.extend(env::split_paths(&paths));
            }
        }
        let ffmpeg = locate("ffmpeg", config.ffmpeg, &directories)?;
        let ffprobe = locate("ffprobe", config.ffprobe, &directories)?;
        let ffmpeg_version = validate(&ffmpeg, "ffmpeg", token, config.validation_timeout)?;
        let ffprobe_version = validate(&ffprobe, "ffprobe", token, config.validation_timeout)?;
        validate_pair_versions(&ffmpeg_version, &ffprobe_version)?;
        Ok(Self {
            ffmpeg,
            ffprobe,
            ffmpeg_version,
            ffprobe_version,
        })
    }
    pub fn ffmpeg(&self) -> &Path {
        &self.ffmpeg
    }
    pub fn ffprobe(&self) -> &Path {
        &self.ffprobe
    }
    pub fn ffmpeg_version(&self) -> &str {
        &self.ffmpeg_version
    }
    pub fn ffprobe_version(&self) -> &str {
        &self.ffprobe_version
    }
}
fn locate(
    name: &'static str,
    explicit: Option<PathBuf>,
    directories: &[PathBuf],
) -> Result<PathBuf> {
    let filename = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.into()
    };
    let candidate = explicit.or_else(|| {
        directories
            .iter()
            .map(|d| d.join(&filename))
            .find(|p| p.is_file())
    });
    match candidate {
        Some(path) if path.is_file() => {
            std::path::absolute(&path).map_err(|e| Error::io("resolve media tool", path, e))
        }
        Some(path) => Err(Error::ToolUnavailable {
            tool: name,
            detail: format!("Configured file is missing: {}", path.display()),
        }),
        None => Err(Error::ToolUnavailable {
            tool: name,
            detail: "No executable in configured, bundle, or PATH directories".into(),
        }),
    }
}
fn validate(
    path: &Path,
    name: &'static str,
    token: &CancellationToken,
    timeout: Duration,
) -> Result<String> {
    let output = run(
        path,
        &["-version".into()],
        token,
        RunOptions {
            operation: "validate media tool",
            timeout: Some(timeout),
            progress_duration: None,
            parse_progress: false,
            events: &(),
        },
    )?;
    let text = String::from_utf8_lossy(&output);
    let first = text.lines().next().unwrap_or_default().trim();
    if !first
        .to_ascii_lowercase()
        .starts_with(&format!("{name} version"))
    {
        return Err(Error::ToolUnavailable {
            tool: name,
            detail: "Executable did not identify itself correctly".into(),
        });
    }
    Ok(first.to_owned())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_missing_override_never_falls_back() {
        let t = tempfile::tempdir().unwrap();
        let name = if cfg!(windows) {
            "ffmpeg.exe"
        } else {
            "ffmpeg"
        };
        std::fs::write(t.path().join(name), b"tool").unwrap();
        assert!(locate(
            "ffmpeg",
            Some(t.path().join("missing")),
            &[t.path().to_owned()]
        )
        .is_err());
        assert_eq!(
            locate("ffmpeg", None, &[t.path().to_owned()]).unwrap(),
            t.path().join(name)
        );
    }
}

fn validate_pair_versions(ffmpeg: &str, ffprobe: &str) -> Result<()> {
    let version = ffmpeg.split_whitespace().nth(2);
    if version.is_none() || version != ffprobe.split_whitespace().nth(2) {
        return Err(Error::ToolUnavailable {
            tool: "ffmpeg/ffprobe",
            detail:
                "Use ffmpeg and ffprobe from one matching build; their version identifiers differ"
                    .into(),
        });
    }
    Ok(())
}
#[cfg(test)]
mod pair_tests {
    use super::*;
    #[test]
    fn a_single_matching_tool_version_is_required() {
        assert!(validate_pair_versions("ffmpeg version 9.0.1", "ffprobe version 9.0.1").is_ok());
        assert!(validate_pair_versions(
            "ffmpeg version 9.0.1-build",
            "ffprobe version 9.0.1-build"
        )
        .is_ok());
        assert!(validate_pair_versions("ffmpeg version 9.0.1", "ffprobe version 7.0.2").is_err());
        assert!(validate_pair_versions("ffmpeg version", "ffprobe version").is_err());
    }
}
