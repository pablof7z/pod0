use std::fs;
use std::path::Path;

use rusqlite::{Connection, MAIN_DB, OpenFlags};

use crate::model::{
    APPLICATION_ID, BackupEvidence, CURRENT_SCHEMA_VERSION, StorageError, command_id,
};
use crate::schema::validate_schema;

pub fn verify_backup(path: &Path) -> Result<BackupEvidence, StorageError> {
    verify_backup_with_reuse(path, false)
}

pub fn restore_backup_to_new_store(
    backup_path: &Path,
    destination_path: &Path,
) -> Result<BackupEvidence, StorageError> {
    if destination_path.exists() {
        return Err(StorageError::BackupConflict);
    }
    let source_evidence = verify_backup(backup_path)?;
    let source = open_read_only(backup_path)?;
    ensure_parent(destination_path)?;
    source
        .backup(MAIN_DB, destination_path, None)
        .map_err(|error| StorageError::sqlite("restore backup", error))?;
    let restored = verify_backup(destination_path)?;
    if restored.schema_version == source_evidence.schema_version
        && restored.store_id == source_evidence.store_id
    {
        Ok(restored)
    } else {
        Err(StorageError::BackupConflict)
    }
}

pub(crate) fn create_or_reuse_backup(
    source: &Connection,
    path: &Path,
    expected_version: u32,
) -> Result<BackupEvidence, StorageError> {
    let expected_store_id = store_identity(source)?;
    if let Some(source_path) = source.path() {
        let source_path = fs::canonicalize(source_path)
            .map_err(|error| StorageError::io("resolve source database path", error))?;
        if path.exists() {
            let backup_path = fs::canonicalize(path)
                .map_err(|error| StorageError::io("resolve backup database path", error))?;
            if source_path == backup_path {
                return Err(StorageError::BackupConflict);
            }
        }
    }
    if path.exists() {
        let evidence = verify_backup_with_reuse(path, true)?;
        return if evidence.schema_version == expected_version
            && evidence.store_id == expected_store_id
        {
            Ok(evidence)
        } else {
            Err(StorageError::BackupConflict)
        };
    }
    ensure_parent(path)?;
    source
        .backup(MAIN_DB, path, None)
        .map_err(|error| StorageError::sqlite("create backup", error))?;
    let evidence = verify_backup_with_reuse(path, false)?;
    if evidence.schema_version == expected_version && evidence.store_id == expected_store_id {
        Ok(evidence)
    } else {
        Err(StorageError::BackupConflict)
    }
}

pub(crate) fn verify_connection(connection: &Connection) -> Result<String, StorageError> {
    let result: String = connection
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(|error| StorageError::sqlite("verify database integrity", error))?;
    if result == "ok" {
        Ok(result)
    } else {
        Err(StorageError::CorruptSchema {
            detail: "SQLite integrity check failed",
        })
    }
}

fn verify_backup_with_reuse(
    path: &Path,
    reused_existing: bool,
) -> Result<BackupEvidence, StorageError> {
    let connection = open_read_only(path)?;
    let integrity_check = verify_connection(&connection)?;
    let application_id: i64 = connection
        .query_row("PRAGMA application_id", [], |row| row.get(0))
        .map_err(|error| StorageError::sqlite("read backup application id", error))?;
    if application_id != APPLICATION_ID {
        return Err(StorageError::BackupConflict);
    }
    let schema_version: u32 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| StorageError::sqlite("read backup schema version", error))?;
    if schema_version > CURRENT_SCHEMA_VERSION {
        return Err(StorageError::NewerSchema {
            stored: schema_version,
            supported: CURRENT_SCHEMA_VERSION,
        });
    }
    validate_schema(&connection, schema_version)?;
    let store_id = store_identity(&connection)?;
    let page_count: i64 = connection
        .query_row("PRAGMA page_count", [], |row| row.get(0))
        .map_err(|error| StorageError::sqlite("read backup page count", error))?;
    let page_count = u64::try_from(page_count).map_err(|_| StorageError::BackupConflict)?;
    let byte_count = fs::metadata(path)
        .map_err(|error| StorageError::io("read backup metadata", error))?
        .len();
    Ok(BackupEvidence {
        path: path.to_path_buf(),
        store_id,
        schema_version,
        byte_count,
        page_count,
        integrity_check,
        reused_existing,
    })
}

fn store_identity(connection: &Connection) -> Result<pod0_domain::CommandId, StorageError> {
    let bytes: Vec<u8> = connection
        .query_row(
            "SELECT store_id FROM pod0_store_metadata WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read store identity", error))?;
    command_id(&bytes)
}

fn open_read_only(path: &Path) -> Result<Connection, StorageError> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
    )
    .map_err(|error| StorageError::sqlite("open read-only database", error))
}

fn ensure_parent(path: &Path) -> Result<(), StorageError> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    fs::create_dir_all(parent).map_err(|error| StorageError::io("create database directory", error))
}
