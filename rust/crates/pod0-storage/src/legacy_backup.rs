use std::fs;
use std::path::Path;

use rusqlite::{Connection, MAIN_DB, OpenFlags};

use crate::StorageError;
use crate::import_model::{LegacyBackupEvidence, LegacyImportPlan, LegacySourceKind};
use crate::legacy_source::inspect_source;

pub(crate) fn create_or_reuse_legacy_backup(
    source_path: &Path,
    backup_path: &Path,
    expected: &LegacyImportPlan,
) -> Result<LegacyBackupEvidence, StorageError> {
    if let Some(parent) = backup_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| StorageError::io("create legacy backup directory", error))?;
    }
    reject_alias(source_path, backup_path)?;
    if backup_path.exists() {
        return verify_evidence(backup_path, expected, true);
    }
    match expected.source_kind {
        LegacySourceKind::SwiftSqlite => backup_sqlite(source_path, backup_path)?,
        LegacySourceKind::LegacyJson => {
            fs::copy(source_path, backup_path)
                .map_err(|error| StorageError::io("copy legacy JSON backup", error))?;
        }
    }
    verify_evidence(backup_path, expected, false)
}

fn backup_sqlite(source_path: &Path, backup_path: &Path) -> Result<(), StorageError> {
    let source = Connection::open_with_flags(
        source_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
    )
    .map_err(|error| StorageError::sqlite("open Swift source for backup", error))?;
    source
        .backup(MAIN_DB, backup_path, None)
        .map_err(|error| StorageError::sqlite("create Swift source backup", error))
}

fn verify_evidence(
    backup_path: &Path,
    expected: &LegacyImportPlan,
    reused_existing: bool,
) -> Result<LegacyBackupEvidence, StorageError> {
    let inspected = inspect_source(backup_path)?;
    if &inspected.plan != expected {
        return Err(StorageError::BackupConflict);
    }
    let byte_count = fs::metadata(backup_path)
        .map_err(|error| StorageError::io("read legacy backup metadata", error))?
        .len();
    Ok(LegacyBackupEvidence {
        source_kind: expected.source_kind,
        source_hash: expected.source_hash.clone(),
        source_generation: expected.source_generation,
        byte_count,
        reused_existing,
    })
}

fn reject_alias(source_path: &Path, backup_path: &Path) -> Result<(), StorageError> {
    let source = fs::canonicalize(source_path)
        .map_err(|error| StorageError::io("resolve legacy source path", error))?;
    if backup_path.exists() {
        let backup = fs::canonicalize(backup_path)
            .map_err(|error| StorageError::io("resolve legacy backup path", error))?;
        if source == backup {
            return Err(StorageError::BackupConflict);
        }
    } else if backup_path == source_path {
        return Err(StorageError::BackupConflict);
    }
    Ok(())
}
