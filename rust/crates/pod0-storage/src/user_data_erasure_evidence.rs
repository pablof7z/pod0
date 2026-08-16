use std::io::Read as _;
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

use crate::StorageError;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct TargetEvidence {
    pub existed: bool,
    pub byte_count: u64,
    pub sha256: Option<String>,
}

pub(crate) fn target_evidence(path: &Path) -> Result<TargetEvidence, StorageError> {
    if !path.exists() {
        return Ok(TargetEvidence {
            existed: false,
            byte_count: 0,
            sha256: None,
        });
    }
    let mut hasher = Sha256::new();
    let mut byte_count = 0;
    hash_entry(path, path, &mut hasher, &mut byte_count)?;
    Ok(TargetEvidence {
        existed: true,
        byte_count,
        sha256: Some(hex(&hasher.finalize())),
    })
}

pub(crate) fn verify_evidence(path: &Path, expected: &TargetEvidence) -> Result<(), StorageError> {
    (target_evidence(path)? == *expected)
        .then_some(())
        .ok_or(StorageError::CommandConflict)
}

fn hash_entry(
    root: &Path,
    path: &Path,
    hasher: &mut Sha256,
    byte_count: &mut u64,
) -> Result<(), StorageError> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| StorageError::io("inspect erasure target", error))?;
    if metadata.file_type().is_symlink() {
        return Err(StorageError::CommandConflict);
    }
    let relative = path
        .strip_prefix(root)
        .map_err(|_| StorageError::CommandConflict)?;
    hash_path(relative, hasher);
    if metadata.is_dir() {
        hasher.update(b"directory\0");
        let mut children = std::fs::read_dir(path)
            .map_err(|error| StorageError::io("enumerate erasure target", error))?
            .map(|entry| {
                entry
                    .map(|value| value.path())
                    .map_err(|error| StorageError::io("read erasure target entry", error))
            })
            .collect::<Result<Vec<PathBuf>, StorageError>>()?;
        children.sort();
        for child in children {
            hash_entry(root, &child, hasher, byte_count)?;
        }
        return Ok(());
    }
    if !metadata.is_file() {
        return Err(StorageError::CommandConflict);
    }
    hasher.update(b"file\0");
    let mut file = std::fs::File::open(path)
        .map_err(|error| StorageError::io("open erasure target", error))?;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| StorageError::io("hash erasure target", error))?;
        if read == 0 {
            break;
        }
        *byte_count = byte_count
            .checked_add(read as u64)
            .ok_or(StorageError::CommandConflict)?;
        hasher.update(&buffer[..read]);
    }
    Ok(())
}

fn hash_path(path: &Path, hasher: &mut Sha256) {
    let value = path.to_string_lossy();
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value.as_bytes());
    hasher.update(b"\0");
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
