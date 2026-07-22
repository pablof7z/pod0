use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use pod0_domain::{DownloadAttemptId, DownloadIntentId};
use sha2::{Digest as _, Sha256};

use crate::StorageError;

pub(crate) struct StagedCopy {
    pub(crate) pending_path: PathBuf,
    pub(crate) byte_count: u64,
    pub(crate) digest: [u8; 32],
}

pub(crate) fn copy_and_hash_staged(
    store: &Path,
    source: &Path,
    attempt_id: DownloadAttemptId,
    claimed_byte_count: u64,
) -> Result<StagedCopy, StorageError> {
    if claimed_byte_count == 0 {
        return Err(StorageError::InvalidDownloadArtifact);
    }
    let metadata = match fs::symlink_metadata(source) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(StorageError::InvalidDownloadArtifact);
        }
        Err(error) => return Err(StorageError::io("inspect staged download", error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(StorageError::InvalidDownloadArtifact);
    }
    let staging_root = download_root(store).join(".staging");
    fs::create_dir_all(&staging_root)
        .map_err(|error| StorageError::io("create download staging directory", error))?;
    let pending_path = pending_artifact_path(store, attempt_id);
    let mut input =
        File::open(source).map_err(|error| StorageError::io("open staged download", error))?;
    let mut output = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&pending_path)
        .map_err(|error| StorageError::io("create durable download staging file", error))?;
    let mut hash = Sha256::new();
    let mut count = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = input
            .read(&mut buffer)
            .map_err(|error| StorageError::io("read staged download", error))?;
        if read == 0 {
            break;
        }
        output
            .write_all(&buffer[..read])
            .map_err(|error| StorageError::io("write durable download staging file", error))?;
        hash.update(&buffer[..read]);
        count = count
            .checked_add(u64::try_from(read).expect("read size fits u64"))
            .ok_or(StorageError::InvalidDownloadArtifact)?;
    }
    output
        .sync_all()
        .map_err(|error| StorageError::io("sync durable download staging file", error))?;
    if count != claimed_byte_count || metadata.len() != claimed_byte_count {
        let _ = fs::remove_file(&pending_path);
        return Err(StorageError::InvalidDownloadArtifact);
    }
    Ok(StagedCopy {
        pending_path,
        byte_count: count,
        digest: hash.finalize().into(),
    })
}

pub(crate) fn pending_artifact_path(store: &Path, attempt_id: DownloadAttemptId) -> PathBuf {
    download_root(store)
        .join(".staging")
        .join(format!("{}.pending", hex(&attempt_id.into_bytes())))
}

pub(crate) fn artifact_path(store: &Path, artifact_key: &str) -> Result<PathBuf, StorageError> {
    let relative = Path::new(artifact_key);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || !artifact_key.starts_with("v1/")
    {
        return Err(StorageError::InvalidDownloadArtifact);
    }
    Ok(download_root(store).join(relative))
}

pub(crate) fn artifact_key(intent: DownloadIntentId, attempt: u16, digest: [u8; 32]) -> String {
    format!(
        "v1/{}-{attempt}-{}.media",
        hex(&intent.into_bytes()),
        hex(&digest)
    )
}

pub(crate) fn install_staged(
    from: &Path,
    to: &Path,
    count: u64,
    digest: [u8; 32],
) -> Result<(), StorageError> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| StorageError::io("create download artifact directory", error))?;
    }
    if to.exists() {
        if verified_file(to, count, digest)? {
            fs::remove_file(from)
                .map_err(|error| StorageError::io("remove duplicate staged download", error))?;
            return Ok(());
        }
        fs::remove_file(to)
            .map_err(|error| StorageError::io("replace invalid download artifact", error))?;
    }
    fs::rename(from, to).map_err(|error| StorageError::io("adopt staged download file", error))?;
    Ok(())
}

pub(crate) fn sync_parent(path: &Path) -> Result<(), StorageError> {
    File::open(path.parent().ok_or(StorageError::InvalidDownloadArtifact)?)
        .and_then(|file| file.sync_all())
        .map_err(|error| StorageError::io("sync download artifact directory", error))
}

pub(crate) fn verified_file(
    path: &Path,
    expected_count: u64,
    expected_digest: [u8; 32],
) -> Result<bool, StorageError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(StorageError::io("inspect download artifact", error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() != expected_count
    {
        return Ok(false);
    }
    let mut file =
        File::open(path).map_err(|error| StorageError::io("open download artifact", error))?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| StorageError::io("verify download artifact", error))?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    Ok(<[u8; 32]>::from(hash.finalize()) == expected_digest)
}

fn download_root(store: &Path) -> PathBuf {
    let name = store
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("pod0-core");
    store
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!("{name}.downloads"))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
