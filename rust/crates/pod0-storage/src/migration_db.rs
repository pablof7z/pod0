use std::fs;
use std::path::Path;

use pod0_domain::CommandId;
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};

use crate::model::{
    APPLICATION_ID, BackupEvidence, CURRENT_SCHEMA_VERSION, StorageError, command_id,
};
use crate::schema::validate_schema;

#[derive(Clone, Copy)]
pub(crate) struct ActiveMigration {
    pub(crate) migration_id: CommandId,
    pub(crate) from_version: u32,
    pub(crate) target_version: u32,
}

pub(crate) fn ensure_parent(path: &Path) -> Result<(), StorageError> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    fs::create_dir_all(parent).map_err(|error| StorageError::io("create store directory", error))
}

pub(crate) fn open_connection(path: &Path, read_only: bool) -> Result<Connection, StorageError> {
    let access = if read_only {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    } else {
        OpenFlags::SQLITE_OPEN_CREATE | OpenFlags::SQLITE_OPEN_READ_WRITE
    };
    Connection::open_with_flags(path, access | OpenFlags::SQLITE_OPEN_FULL_MUTEX)
        .map_err(|error| StorageError::sqlite("open core store", error))
}

pub(crate) fn configure(connection: &Connection) -> Result<(), StorageError> {
    connection
        .busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|error| StorageError::sqlite("set busy timeout", error))?;
    connection
        .execute_batch("PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL;")
        .map_err(|error| StorageError::sqlite("configure core store", error))
}

pub(crate) fn enable_write_ahead_logging(connection: &Connection) -> Result<(), StorageError> {
    let journal_mode: String = connection
        .query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))
        .map_err(|error| StorageError::sqlite("enable write-ahead logging", error))?;
    if journal_mode.eq_ignore_ascii_case("wal") {
        Ok(())
    } else {
        Err(StorageError::Sqlite {
            operation: "enable write-ahead logging",
        })
    }
}

pub(crate) fn user_version(connection: &Connection) -> Result<u32, StorageError> {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| StorageError::sqlite("read schema version", error))
}

pub(crate) fn application_id(connection: &Connection) -> Result<i64, StorageError> {
    connection
        .query_row("PRAGMA application_id", [], |row| row.get(0))
        .map_err(|error| StorageError::sqlite("read application id", error))
}

pub(crate) fn validate_open_database(
    connection: &Connection,
    version: u32,
) -> Result<(), StorageError> {
    if version > CURRENT_SCHEMA_VERSION {
        return Err(StorageError::NewerSchema {
            stored: version,
            supported: CURRENT_SCHEMA_VERSION,
        });
    }
    let identifier = application_id(connection)?;
    if version == 0 {
        if identifier != 0 && identifier != APPLICATION_ID {
            return Err(StorageError::ForeignDatabase);
        }
    } else if identifier != APPLICATION_ID {
        return Err(StorageError::ForeignDatabase);
    }
    validate_schema(connection, version)
}

pub(crate) fn active_migration(
    connection: &Connection,
) -> Result<Option<ActiveMigration>, StorageError> {
    let stored = user_version(connection)?;
    let row = connection
        .query_row(
            "SELECT migration_id,from_version,to_version FROM pod0_migration_journal \
             WHERE state='running' AND to_version>?1 ORDER BY started_at_ms LIMIT 1",
            [stored],
            |row| {
                let bytes: Vec<u8> = row.get(0)?;
                Ok((bytes, row.get(1)?, row.get(2)?))
            },
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read active migration", error))?;
    row.map(|(bytes, from_version, target_version)| {
        Ok(ActiveMigration {
            migration_id: command_id(&bytes)?,
            from_version,
            target_version,
        })
    })
    .transpose()
}

pub(crate) fn unfinished_migration(
    connection: &Connection,
    state: &str,
) -> Result<Option<(u32, u32)>, StorageError> {
    let stored = user_version(connection)?;
    connection
        .query_row(
            "SELECT from_version,to_version FROM pod0_migration_journal \
             WHERE state=?1 AND to_version>?2 ORDER BY started_at_ms LIMIT 1",
            params![state, stored],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read unfinished migration", error))
}

pub(crate) fn reconcile_committed_journal(
    connection: &Connection,
    completed_at_ms: i64,
) -> Result<usize, StorageError> {
    let stored = user_version(connection)?;
    let updated = connection
        .execute(
            "UPDATE pod0_migration_journal SET state='completed',completed_at_ms=?1,diagnostic_code=NULL \
             WHERE state='running' AND to_version<=?2",
            params![completed_at_ms, stored],
        )
        .map_err(|error| StorageError::sqlite("reconcile migration journal", error))?;
    Ok(updated)
}

pub(crate) fn start_journal(
    connection: &Connection,
    migration_id: CommandId,
    from_version: u32,
    to_version: u32,
    started_at_ms: i64,
) -> Result<(), StorageError> {
    connection
        .execute(
            "INSERT INTO pod0_migration_journal( \
                migration_id,from_version,to_version,state,started_at_ms \
             ) VALUES(?1,?2,?3,'running',?4) \
             ON CONFLICT(migration_id,to_version) DO NOTHING",
            params![
                migration_id.into_bytes().as_slice(),
                from_version,
                to_version,
                started_at_ms
            ],
        )
        .map_err(|error| StorageError::sqlite("start migration journal", error))?;
    let state: String = connection
        .query_row(
            "SELECT state FROM pod0_migration_journal WHERE migration_id=?1 AND to_version=?2",
            params![migration_id.into_bytes().as_slice(), to_version],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("verify migration journal", error))?;
    if state == "running" {
        Ok(())
    } else {
        Err(StorageError::CorruptSchema {
            detail: "migration journal state conflicts with stored version",
        })
    }
}

pub(crate) fn complete_journal(
    connection: &Connection,
    migration_id: CommandId,
    from_version: u32,
    to_version: u32,
    completed_at_ms: i64,
) -> Result<(), StorageError> {
    connection
        .execute(
            "INSERT INTO pod0_migration_journal( \
                migration_id,from_version,to_version,state,started_at_ms,completed_at_ms \
             ) VALUES(?1,?2,?3,'completed',?4,?4) \
             ON CONFLICT(migration_id,to_version) DO UPDATE SET \
                state='completed',completed_at_ms=excluded.completed_at_ms,diagnostic_code=NULL",
            params![
                migration_id.into_bytes().as_slice(),
                from_version,
                to_version,
                completed_at_ms
            ],
        )
        .map_err(|error| StorageError::sqlite("complete migration journal", error))?;
    Ok(())
}

pub(crate) fn fail_journal(
    connection: &Connection,
    migration_id: CommandId,
    to_version: u32,
    diagnostic_code: &str,
    completed_at_ms: i64,
) -> Result<(), StorageError> {
    connection
        .execute(
            "UPDATE pod0_migration_journal SET state='failed',completed_at_ms=?1,diagnostic_code=?2 \
             WHERE migration_id=?3 AND to_version=?4 AND state='running'",
            params![
                completed_at_ms,
                diagnostic_code,
                migration_id.into_bytes().as_slice(),
                to_version
            ],
        )
        .map_err(|error| StorageError::sqlite("fail migration journal", error))?;
    Ok(())
}

pub(crate) fn record_backup(
    connection: &Connection,
    migration_id: CommandId,
    evidence: &BackupEvidence,
    created_at_ms: i64,
) -> Result<(), StorageError> {
    let byte_count =
        i64::try_from(evidence.byte_count).map_err(|_| StorageError::BackupConflict)?;
    let page_count =
        i64::try_from(evidence.page_count).map_err(|_| StorageError::BackupConflict)?;
    connection
        .execute(
            "INSERT INTO pod0_backup_evidence( \
                migration_id,store_id,schema_version,byte_count,page_count,integrity_check,created_at_ms \
             ) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(migration_id) DO NOTHING",
            params![
                migration_id.into_bytes().as_slice(),
                evidence.store_id.into_bytes().as_slice(),
                evidence.schema_version,
                byte_count,
                page_count,
                evidence.integrity_check.as_str(),
                created_at_ms
            ],
        )
        .map_err(|error| StorageError::sqlite("record backup evidence", error))?;
    let stored: (Vec<u8>, u32, i64, i64, String) = connection
        .query_row(
            "SELECT store_id,schema_version,byte_count,page_count,integrity_check \
             FROM pod0_backup_evidence WHERE migration_id=?1",
            [migration_id.into_bytes().as_slice()],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .map_err(|error| StorageError::sqlite("verify backup evidence", error))?;
    if stored
        == (
            evidence.store_id.into_bytes().to_vec(),
            evidence.schema_version,
            byte_count,
            page_count,
            evidence.integrity_check.clone(),
        )
    {
        Ok(())
    } else {
        Err(StorageError::BackupConflict)
    }
}
