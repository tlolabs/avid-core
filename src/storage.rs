use crate::{Error, Result};
#[cfg(windows)]
use std::time::{Duration, Instant};
#[cfg(all(test, not(windows)))]
use std::time::{Duration, Instant};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;
pub(crate) fn absolute(path: &Path) -> Result<PathBuf> {
    std::path::absolute(path).map_err(|e| Error::io("resolve path", path, e))
}
pub(crate) fn file(path: &Path) -> Result<()> {
    if path.is_file() {
        Ok(())
    } else {
        Err(Error::InvalidInput(format!(
            "Source is missing or is not a file: {}",
            path.display()
        )))
    }
}
pub(crate) fn protect(
    output: &Path,
    inputs: impl IntoIterator<Item = impl AsRef<Path>>,
) -> Result<()> {
    let output_exists = output.exists();
    if output_exists && !output.is_file() {
        return Err(Error::InvalidInput(
            "Destination must be a file path".into(),
        ));
    }
    let abs_output = absolute(output)?;
    for input in inputs {
        let input = input.as_ref();
        let aliases = absolute(input)? == abs_output
            || (output_exists
                && input.exists()
                && same_file::is_same_file(input, output)
                    .map_err(|e| Error::io("compare destination and source", output, e))?);
        if aliases {
            return Err(Error::InvalidInput(format!(
                "Destination must not replace a source or protected file: {}",
                input.display()
            )));
        }
    }
    Ok(())
}
pub(crate) struct StagedOutput {
    file: NamedTempFile,
}
impl StagedOutput {
    pub fn new(destination: &Path, suffix: &str) -> Result<Self> {
        let parent = destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent).map_err(|e| Error::io("create output folder", parent, e))?;
        let mut builder = tempfile::Builder::new();
        builder.prefix(".avid-").suffix(suffix);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // Match FFmpeg's ordinary output creation mode, respecting the process umask.
            builder.permissions(fs::Permissions::from_mode(0o666));
        }
        let file = builder
            .tempfile_in(parent)
            .map_err(|e| Error::io("stage output", destination, e))?;
        Ok(Self { file })
    }
    pub fn path(&self) -> &Path {
        self.file.path()
    }
    pub fn publish(self, destination: &Path) -> Result<()> {
        // Sync the actual pathname written by FFmpeg before publication.
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(self.path())
            .map_err(|e| Error::io("open completed output", self.path(), e))?;
        if file
            .metadata()
            .map_err(|e| Error::io("inspect completed output", self.path(), e))?
            .len()
            == 0
        {
            return Err(Error::InvalidInput(
                "Media tool produced an empty output".into(),
            ));
        }
        file.sync_all()
            .map_err(|e| Error::io("flush completed output", self.path(), e))?;
        drop(file);
        // tempfile uses POSIX rename / Windows MoveFileExW(REPLACE_EXISTING).
        // A persist error retains ownership, so dropping the error also cleans the stage.
        persist_with_retry(self.file, destination)
            .map_err(|error| Error::io("publish completed output", destination, error.error))?;
        Ok(())
    }
}

fn persist_with_retry(
    file: NamedTempFile,
    destination: &Path,
) -> std::result::Result<(), tempfile::PersistError> {
    #[cfg(windows)]
    {
        return retry_persist(
            file,
            destination,
            Instant::now() + Duration::from_secs(2),
            |file, destination| file.persist(destination),
        );
    }
    #[cfg(not(windows))]
    file.persist(destination).map(|_| ())
}

#[cfg(any(windows, test))]
fn retry_persist(
    mut file: NamedTempFile,
    destination: &Path,
    deadline: Instant,
    mut persist: impl FnMut(
        NamedTempFile,
        &Path,
    ) -> std::result::Result<fs::File, tempfile::PersistError>,
) -> std::result::Result<(), tempfile::PersistError> {
    let mut delay = Duration::from_millis(5);
    loop {
        match persist(file, destination) {
            Ok(_) => return Ok(()),
            Err(error) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if retryable_publish_error(&error.error) && remaining > delay {
                    file = error.file;
                    std::thread::sleep(delay);
                    delay = (delay * 2).min(Duration::from_millis(50));
                    continue;
                }
                return Err(error);
            }
        }
    }
}

#[cfg(any(windows, test))]
fn retryable_publish_error(error: &std::io::Error) -> bool {
    matches!(error.raw_os_error(), Some(5 | 32 | 33))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stage_cleanup_publish_and_protection() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("final.mp4");
        fs::write(&destination, b"old").unwrap();
        let path = {
            let staged = StagedOutput::new(&destination, ".mp4").unwrap();
            staged.path().to_owned()
        };
        assert!(!path.exists());
        let staged = StagedOutput::new(&destination, ".mp4").unwrap();
        fs::write(staged.path(), b"new").unwrap();
        staged.publish(&destination).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"new");
        let alias = root.path().join("alias.mp4");
        fs::hard_link(&destination, &alias).unwrap();
        assert!(protect(&alias, [&destination]).is_err());
        let staged = StagedOutput::new(&destination, ".mp4").unwrap();
        assert!(staged.publish(&destination).is_err());
        assert_eq!(fs::read(&destination).unwrap(), b"new");
    }
    #[test]
    fn failed_publication_cleans_stage() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("folder");
        fs::create_dir(&destination).unwrap();
        let stage = StagedOutput::new(&destination, ".mp4").unwrap();
        let path = stage.path().to_owned();
        fs::write(&path, b"video").unwrap();
        assert!(stage.publish(&destination).is_err());
        assert!(!path.exists());
        assert!(destination.is_dir());
    }

    #[test]
    fn only_transient_windows_publish_errors_are_retryable() {
        for code in [5, 32, 33] {
            assert!(retryable_publish_error(&std::io::Error::from_raw_os_error(
                code
            )));
        }
        for code in [1, 2, 13] {
            assert!(!retryable_publish_error(
                &std::io::Error::from_raw_os_error(code)
            ));
        }
    }

    #[test]
    fn injected_transient_publish_error_retries_and_preserves_stage_on_failure() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("final.mp4");
        fs::write(&destination, b"old").unwrap();
        let staged = NamedTempFile::new_in(root.path()).unwrap();
        let stage_path = staged.path().to_owned();
        fs::write(&stage_path, b"new").unwrap();
        let attempts = std::cell::Cell::new(0);
        retry_persist(
            staged,
            &destination,
            Instant::now() + Duration::from_secs(2),
            |file, destination| {
                attempts.set(attempts.get() + 1);
                if attempts.get() == 1 {
                    Err(tempfile::PersistError {
                        error: std::io::Error::from_raw_os_error(32),
                        file,
                    })
                } else {
                    file.persist(destination)
                }
            },
        )
        .unwrap();
        assert_eq!(attempts.get(), 2);
        assert_eq!(fs::read(&destination).unwrap(), b"new");
        assert!(!stage_path.exists());

        fs::write(&destination, b"preserved").unwrap();
        let staged = NamedTempFile::new_in(root.path()).unwrap();
        let stage_path = staged.path().to_owned();
        fs::write(&stage_path, b"discard").unwrap();
        let attempts = std::cell::Cell::new(0);
        let error = retry_persist(
            staged,
            &destination,
            Instant::now() + Duration::from_secs(2),
            |file, _| {
                attempts.set(attempts.get() + 1);
                Err(tempfile::PersistError {
                    error: std::io::Error::from_raw_os_error(13),
                    file,
                })
            },
        )
        .unwrap_err();
        assert_eq!(attempts.get(), 1);
        assert_eq!(fs::read(&destination).unwrap(), b"preserved");
        let failed_stage = error.file.path().to_owned();
        drop(error);
        assert!(!failed_stage.exists());
    }
    #[test]
    fn exhausted_transient_publish_retries_preserve_destination_and_clean_stage() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("final.mp4");
        fs::write(&destination, b"preserved").unwrap();
        let staged = NamedTempFile::new_in(root.path()).unwrap();
        let stage_path = staged.path().to_owned();
        fs::write(&stage_path, b"discard").unwrap();
        let attempts = std::cell::Cell::new(0);
        let error = retry_persist(
            staged,
            &destination,
            Instant::now() + Duration::from_millis(20),
            |file, _| {
                attempts.set(attempts.get() + 1);
                Err(tempfile::PersistError {
                    error: std::io::Error::from_raw_os_error(32),
                    file,
                })
            },
        )
        .unwrap_err();
        // The 5ms and 10ms waits permit at most three attempts in this budget.
        assert!((1..=3).contains(&attempts.get()));
        assert_eq!(error.error.raw_os_error(), Some(32));
        assert_eq!(fs::read(&destination).unwrap(), b"preserved");
        drop(error);
        assert!(!stage_path.exists());
    }
}

#[cfg(all(test, unix))]
mod permission_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn output_creation_mode_matches_normal_files_without_changing_umask() {
        let root = tempfile::tempdir().unwrap();
        let ordinary = root.path().join("ordinary");
        fs::write(&ordinary, b"ordinary").unwrap();
        let stage = StagedOutput::new(&root.path().join("output.mp4"), ".mp4").unwrap();
        assert_eq!(
            fs::metadata(stage.path()).unwrap().permissions().mode() & 0o777,
            fs::metadata(ordinary).unwrap().permissions().mode() & 0o777
        );
    }
}
