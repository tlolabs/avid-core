use crate::{Error, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const WHISPER_REVISION: &str = "4979e04f5dcaccb36057e059bbaed8a2f5288315";
pub const WHISPER_SOURCE_SHA256: &str =
    "ec8e33f4c838ed459ad0a23da255c0262f150a6f54839a17e504ddf1495305d5";

pub(crate) fn pinned_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}
pub(crate) fn identity_component(value: &str) -> bool {
    value
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_alphanumeric)
        && value
            .bytes()
            .all(|v| v.is_ascii_alphanumeric() || b"-_.".contains(&v))
}
fn repository(value: &str) -> bool {
    value.split('/').count() == 2 && value.split('/').all(identity_component)
}
fn relative_artifact_path(value: &str) -> bool {
    !value.bytes().any(|v| v < 32 || b"\\:".contains(&v))
        && value.split('/').all(|component| {
            !component.is_empty()
                && component != "."
                && component != ".."
                && !component.starts_with(".avid-")
        })
}

/// Exact repository bytes used by a model, including independently pinned
/// tokenizers or other companion files when a provider requires them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelArtifactIdentity {
    pub path: String,
    pub repository: String,
    pub revision: String,
    pub repository_path: String,
    pub size_bytes: u64,
    pub sha256: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelIdentity {
    pub schema_version: u32,
    #[serde(rename = "installationID")]
    pub installation_id: String,
    pub repository: String,
    pub revision: String,
    #[serde(rename = "manifestSHA256")]
    pub manifest_sha256: String,
    pub artifacts: Vec<ModelArtifactIdentity>,
}
impl ModelIdentity {
    pub fn validate(&self) -> Result<()> {
        let mut paths = std::collections::HashSet::new();
        if self.schema_version != 1
            || !repository(&self.repository)
            || !pinned_hex(&self.revision, 40)
            || !pinned_hex(&self.manifest_sha256, 64)
            || !(self.installation_id == format!("{}-{}", self.revision, self.manifest_sha256)
                || self.installation_id
                    == format!("{}-{}", self.revision, &self.manifest_sha256[..16]))
            || self.artifacts.is_empty()
            || self.artifacts.iter().any(|artifact| {
                !relative_artifact_path(&artifact.path)
                    || !paths.insert(artifact.path.to_lowercase())
                    || !repository(&artifact.repository)
                    || !pinned_hex(&artifact.revision, 40)
                    || !relative_artifact_path(&artifact.repository_path)
                    || artifact.size_bytes > i64::MAX as u64
                    || !pinned_hex(&artifact.sha256, 64)
            })
        {
            return Err(Error::InvalidInput("invalid pinned model identity".into()));
        }
        if paths.iter().any(|path| {
            path.match_indices('/')
                .any(|(index, _)| paths.contains(&path[..index]))
        }) {
            return Err(Error::InvalidInput(
                "model identity artifact paths collide as files and directories".into(),
            ));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelManifest {
    pub id: String,
    pub runtime: String,
    pub format: String,
    pub repository: String,
    pub revision: String,
    pub file: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub model_license: String,
    /// License of the pinned upstream base SDK. AVID runtime safety patches and
    /// their combined distribution rights belong to the separate SDK build
    /// receipt; administrative SDK changes do not change model installation IDs.
    pub sdk_license: String,
}
impl ModelManifest {
    pub fn tiny_english() -> Self {
        Self {
            id: "whisper-cpp-tiny-en".into(),
            runtime: "whisper.cpp-1.8.2".into(),
            format: "ggml".into(),
            repository: "ggerganov/whisper.cpp".into(),
            revision: "5359861c739e955e79d9a303bcbc70fb988958b1".into(),
            file: "ggml-tiny.en.bin".into(),
            size_bytes: 77_704_715,
            sha256: "921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f".into(),
            model_license: "MIT (OpenAI Whisper weights; preserve upstream notice)".into(),
            sdk_license: "MIT (ggml/whisper.cpp)".into(),
        }
    }
    pub fn validate(&self) -> Result<()> {
        let hex = |v: &str, len| {
            v.len() == len
                && v.bytes()
                    .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
        };
        let component = |v: &str| {
            !v.is_empty()
                && !v.starts_with('.')
                && v.bytes()
                    .all(|v| v.is_ascii_alphanumeric() || b"-_.".contains(&v))
        };
        if !component(&self.id)
            || !component(&self.file)
            || !hex(&self.revision, 40)
            || !hex(&self.sha256, 64)
            || self.size_bytes == 0
            || self.size_bytes > 8_000_000_000
            || self.runtime != "whisper.cpp-1.8.2"
            || self.format != "ggml"
            || self.repository.split('/').count() != 2
            || !self.repository.split('/').all(component)
            || self.model_license.is_empty()
            || self.sdk_license.is_empty()
        {
            return Err(Error::Model("invalid pinned GGML model manifest".into()));
        }
        Ok(())
    }
    /// Includes every manifest field, so the same model revision cannot alias a
    /// different conversion/hash/license selection.
    pub fn installation_id(&self) -> Result<String> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|v| Error::Model(v.to_string()))?;
        let digest: String = Sha256::digest(bytes)
            .iter()
            .map(|v| format!("{v:02x}"))
            .collect();
        Ok(format!("{}-{digest}", self.revision))
    }
    /// GGML stores the tokenizer with the weights, so its complete verified
    /// file hash covers both. Keep the existing complete-manifest cache ID.
    pub fn identity(&self) -> Result<ModelIdentity> {
        let installation_id = self.installation_id()?;
        let manifest_sha256 = installation_id[self.revision.len() + 1..].to_owned();
        let identity = ModelIdentity {
            schema_version: 1,
            installation_id,
            repository: self.repository.clone(),
            revision: self.revision.clone(),
            manifest_sha256,
            artifacts: vec![ModelArtifactIdentity {
                path: self.file.clone(),
                repository: self.repository.clone(),
                revision: self.revision.clone(),
                repository_path: self.file.clone(),
                size_bytes: self.size_bytes,
                sha256: self.sha256.clone(),
            }],
        };
        identity.validate()?;
        Ok(identity)
    }
}
/// Host chooses a sandbox/cache root. No network or implicit model acquisition
/// occurs through this API. Installed directories are immutable under a lease.
pub struct ModelStore {
    root: PathBuf,
}
pub struct ModelLease {
    manifest: ModelManifest,
    path: PathBuf,
    _lock: File,
}
impl ModelLease {
    pub fn manifest(&self) -> &ModelManifest {
        &self.manifest
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
}
fn check(cancelled: &dyn Fn() -> bool) -> Result<()> {
    if cancelled() {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}
fn no_symlink(path: &Path) -> Result<()> {
    if fs::symlink_metadata(path).is_ok_and(|v| v.file_type().is_symlink()) {
        return Err(Error::Model(format!(
            "model path is a symlink: {}",
            path.display()
        )));
    }
    Ok(())
}
fn verify(path: &Path, manifest: &ModelManifest, cancelled: &dyn Fn() -> bool) -> Result<()> {
    check(cancelled)?;
    no_symlink(path)?;
    let mut input = File::open(path)?;
    let metadata = input.metadata()?;
    if !metadata.is_file() || metadata.len() != manifest.size_bytes {
        return Err(Error::Model(
            "model file size/type does not match pinned manifest".into(),
        ));
    }
    let mut hash = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    let mut total = 0u64;
    loop {
        check(cancelled)?;
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > manifest.size_bytes {
            return Err(Error::Model("model grew during verification".into()));
        }
        hash.update(&buffer[..count]);
    }
    let digest: String = hash.finalize().iter().map(|v| format!("{v:02x}")).collect();
    if total != manifest.size_bytes || digest != manifest.sha256 {
        return Err(Error::Model("model SHA-256 verification failed".into()));
    }
    Ok(())
}
impl ModelStore {
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        fs::create_dir_all(root.as_ref())?;
        no_symlink(root.as_ref())?;
        Ok(Self {
            root: root.as_ref().canonicalize()?,
        })
    }
    fn lock(&self, manifest: &ModelManifest, exclusive: bool) -> Result<File> {
        manifest.validate()?;
        let directory = self.root.join(".avid-locks");
        no_symlink(&directory)?;
        fs::create_dir_all(&directory)?;
        let path = directory.join(format!("{}.lock", manifest.id));
        no_symlink(&path)?;
        // Lock inodes are persistent: unlinking a locked file would break exclusion.
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        (if exclusive {
            FileExt::try_lock_exclusive(&file)
        } else {
            FileExt::try_lock_shared(&file)
        })
        .map_err(|v| {
            if v.kind() == std::io::ErrorKind::WouldBlock {
                Error::Busy
            } else {
                Error::Io(v)
            }
        })?;
        Ok(file)
    }
    fn directory(&self, manifest: &ModelManifest) -> Result<PathBuf> {
        Ok(self
            .root
            .join(&manifest.id)
            .join(manifest.installation_id()?))
    }
    /// Explicitly import exact model bytes. Copy+verify prevents external sources
    /// or extra directory files from changing the installed model definition.
    pub fn import(
        &self,
        manifest: &ModelManifest,
        source: &Path,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<()> {
        let _lock = self.lock(manifest, true)?;
        verify(source, manifest, cancelled)?;
        let parent = self.root.join(&manifest.id);
        no_symlink(&parent)?;
        fs::create_dir_all(&parent)?;
        let destination = self.directory(manifest)?;
        no_symlink(&destination)?;
        if destination.exists() {
            return verify(&destination.join(&manifest.file), manifest, cancelled);
        }
        let staging = tempfile::Builder::new()
            .prefix(".avid-import-")
            .tempdir_in(&parent)?;
        let output = staging.path().join(&manifest.file);
        let mut input = File::open(source)?;
        let mut output_file = File::create(&output)?;
        let mut buffer = vec![0u8; 1024 * 1024];
        let mut total = 0u64;
        loop {
            check(cancelled)?;
            let count = input.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            total += count as u64;
            if total > manifest.size_bytes {
                return Err(Error::Model("model source grew during import".into()));
            }
            output_file.write_all(&buffer[..count])?;
        }
        output_file.sync_all()?;
        drop(output_file);
        verify(&output, manifest, cancelled)?;
        let receipt =
            serde_json::to_vec_pretty(manifest).map_err(|v| Error::Model(v.to_string()))?;
        fs::write(staging.path().join("model.json"), receipt)?;
        check(cancelled)?;
        fs::rename(staging.path(), destination)?;
        Ok(())
    }
    /// Verify every byte under the persistent cross-process lease. Keep this
    /// value alive through model loading, inference and unload.
    pub fn lease(
        &self,
        manifest: &ModelManifest,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<ModelLease> {
        let lock = self.lock(manifest, false)?;
        let directory = self.directory(manifest)?;
        no_symlink(&self.root.join(&manifest.id))?;
        no_symlink(&directory)?;
        let receipt = fs::read(directory.join("model.json"))?;
        let installed: ModelManifest =
            serde_json::from_slice(&receipt).map_err(|v| Error::Model(v.to_string()))?;
        if &installed != manifest {
            return Err(Error::Model(
                "model receipt differs from requested immutable identity".into(),
            ));
        }
        let path = directory.join(&manifest.file);
        verify(&path, manifest, cancelled)?;
        Ok(ModelLease {
            manifest: manifest.clone(),
            path,
            _lock: lock,
        })
    }
    pub fn remove(&self, manifest: &ModelManifest) -> Result<()> {
        let _lock = self.lock(manifest, true)?;
        let directory = self.root.join(&manifest.id);
        no_symlink(&directory)?;
        if directory.exists() {
            fs::remove_dir_all(directory)?;
        }
        Ok(())
    }
}
