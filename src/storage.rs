use crate::{Error, Result};
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
    if output.exists() && !output.is_file() {
        return Err(Error::InvalidInput(
            "Destination must be a file path".into(),
        ));
    }
    for input in inputs {
        let input = input.as_ref();
        let aliases = absolute(input)? == absolute(output)?
            || (input.exists()
                && output.exists()
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
        // tempfile uses POSIX rename / Windows MoveFileExW(REPLACE_EXISTING).
        // A persist error retains ownership, so dropping the error also cleans the stage.
        self.file
            .persist(destination)
            .map_err(|error| Error::io("publish completed output", destination, error.error))?;
        Ok(())
    }
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
