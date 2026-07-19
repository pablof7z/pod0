use std::fs;
use std::path::Path;

use rusqlite::{Connection, MAIN_DB, OpenFlags};

use crate::legacy_clip_source::inspect_clip_source;
use crate::{ClipBackupEvidence, ClipImportPlan, LegacySourceKind, StorageError};

pub(crate) fn create_or_reuse_clip_backup(
    source_path: &Path,
    backup_path: &Path,
    expected: &ClipImportPlan,
) -> Result<ClipBackupEvidence, StorageError> {
    if let Some(parent) = backup_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| StorageError::io("create clip backup directory", error))?;
    }
    reject_alias(source_path, backup_path)?;
    if backup_path.exists() {
        return verify(backup_path, expected, true);
    }
    match expected.source_kind {
        LegacySourceKind::SwiftSqlite => {
            let source = Connection::open_with_flags(
                source_path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
            )
            .map_err(|error| StorageError::sqlite("open clip source for backup", error))?;
            source
                .backup(MAIN_DB, backup_path, None)
                .map_err(|error| StorageError::sqlite("create clip source backup", error))?;
        }
        LegacySourceKind::LegacyJson => {
            fs::copy(source_path, backup_path)
                .map_err(|error| StorageError::io("copy clip JSON backup", error))?;
        }
    }
    verify(backup_path, expected, false)
}

fn verify(
    backup_path: &Path,
    expected: &ClipImportPlan,
    reused_existing: bool,
) -> Result<ClipBackupEvidence, StorageError> {
    if inspect_clip_source(backup_path)?.plan != *expected {
        return Err(StorageError::BackupConflict);
    }
    let byte_count = fs::metadata(backup_path)
        .map_err(|error| StorageError::io("read clip backup metadata", error))?
        .len();
    Ok(ClipBackupEvidence {
        source_kind: expected.source_kind,
        source_hash: expected.source_hash.clone(),
        source_generation: expected.source_generation,
        byte_count,
        reused_existing,
    })
}

fn reject_alias(source_path: &Path, backup_path: &Path) -> Result<(), StorageError> {
    let source = fs::canonicalize(source_path)
        .map_err(|error| StorageError::io("resolve clip source path", error))?;
    if backup_path.exists() {
        let backup = fs::canonicalize(backup_path)
            .map_err(|error| StorageError::io("resolve clip backup path", error))?;
        if source == backup {
            return Err(StorageError::BackupConflict);
        }
    } else if backup_path == source_path {
        return Err(StorageError::BackupConflict);
    }
    Ok(())
}
