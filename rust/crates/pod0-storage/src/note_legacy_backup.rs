use std::fs;
use std::path::Path;

use rusqlite::{Connection, MAIN_DB, OpenFlags};

use crate::legacy_note_source::inspect_note_source;
use crate::{LegacySourceKind, NoteBackupEvidence, NoteImportPlan, StorageError};

pub(crate) fn create_or_reuse_note_backup(
    source_path: &Path,
    backup_path: &Path,
    expected: &NoteImportPlan,
) -> Result<NoteBackupEvidence, StorageError> {
    if let Some(parent) = backup_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| StorageError::io("create note backup directory", error))?;
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
            .map_err(|error| StorageError::sqlite("open note source for backup", error))?;
            source
                .backup(MAIN_DB, backup_path, None)
                .map_err(|error| StorageError::sqlite("create note source backup", error))?;
        }
        LegacySourceKind::LegacyJson => {
            fs::copy(source_path, backup_path)
                .map_err(|error| StorageError::io("copy note JSON backup", error))?;
        }
    }
    verify(backup_path, expected, false)
}

fn verify(
    backup_path: &Path,
    expected: &NoteImportPlan,
    reused_existing: bool,
) -> Result<NoteBackupEvidence, StorageError> {
    if inspect_note_source(backup_path)?.plan != *expected {
        return Err(StorageError::BackupConflict);
    }
    let byte_count = fs::metadata(backup_path)
        .map_err(|error| StorageError::io("read note backup metadata", error))?
        .len();
    Ok(NoteBackupEvidence {
        source_kind: expected.source_kind,
        source_hash: expected.source_hash.clone(),
        source_generation: expected.source_generation,
        byte_count,
        reused_existing,
    })
}

fn reject_alias(source_path: &Path, backup_path: &Path) -> Result<(), StorageError> {
    let source = fs::canonicalize(source_path)
        .map_err(|error| StorageError::io("resolve note source path", error))?;
    if backup_path.exists() {
        let backup = fs::canonicalize(backup_path)
            .map_err(|error| StorageError::io("resolve note backup path", error))?;
        if source == backup {
            return Err(StorageError::BackupConflict);
        }
    } else if backup_path == source_path {
        return Err(StorageError::BackupConflict);
    }
    Ok(())
}
