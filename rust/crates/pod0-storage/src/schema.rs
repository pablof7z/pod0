use std::collections::BTreeSet;

use pod0_domain::CommandId;
use rusqlite::{Connection, Transaction, params};

use crate::model::{APPLICATION_ID, StorageError};

const MIGRATION_1: &str = include_str!("../../../schema/migrations/0001_kernel_metadata.sql");
const MIGRATION_2: &str = include_str!("../../../schema/migrations/0002_migration_journal.sql");
const MIGRATION_3: &str = include_str!("../../../schema/migrations/0003_domain_cutovers.sql");

pub(crate) fn migration_sql(version: u32) -> Option<&'static str> {
    match version {
        1 => Some(MIGRATION_1),
        2 => Some(MIGRATION_2),
        3 => Some(MIGRATION_3),
        _ => None,
    }
}

pub(crate) fn apply_step(
    transaction: &Transaction<'_>,
    version: u32,
    observed_at_ms: i64,
    store_id: CommandId,
) -> Result<(), StorageError> {
    let sql = migration_sql(version).ok_or(StorageError::CorruptSchema {
        detail: "missing migration step",
    })?;
    transaction
        .execute_batch(sql)
        .map_err(|error| StorageError::sqlite("apply schema step", error))?;
    if version == 1 {
        transaction
            .execute(
                "INSERT INTO pod0_store_metadata(singleton,store_id) VALUES(1,?1)",
                [store_id.into_bytes().as_slice()],
            )
            .map_err(|error| StorageError::sqlite("record store identity", error))?;
    }
    transaction
        .pragma_update(None, "application_id", APPLICATION_ID)
        .map_err(|error| StorageError::sqlite("set application id", error))?;
    transaction
        .pragma_update(None, "user_version", version)
        .map_err(|error| StorageError::sqlite("set schema version", error))?;
    transaction
        .execute(
            "INSERT INTO pod0_schema_versions(component,version,updated_at_ms) VALUES('kernel',?1,?2) \
             ON CONFLICT(component) DO UPDATE SET version=excluded.version,updated_at_ms=excluded.updated_at_ms",
            params![version, observed_at_ms],
        )
        .map_err(|error| StorageError::sqlite("record component version", error))?;
    Ok(())
}

pub(crate) fn validate_schema(connection: &Connection, version: u32) -> Result<(), StorageError> {
    if version == 0 {
        let tables = table_names(connection)?;
        if tables.is_empty() {
            return Ok(());
        }
        return Err(StorageError::ForeignDatabase);
    }
    require_columns(
        connection,
        "pod0_schema_versions",
        &["component", "updated_at_ms", "version"],
    )?;
    require_columns(
        connection,
        "pod0_store_metadata",
        &["singleton", "store_id"],
    )?;
    let identity_count: u32 = connection
        .query_row("SELECT COUNT(*) FROM pod0_store_metadata", [], |row| {
            row.get(0)
        })
        .map_err(|error| StorageError::sqlite("validate store identity", error))?;
    if identity_count != 1 {
        return Err(StorageError::CorruptSchema {
            detail: "store identity must contain one row",
        });
    }
    let recorded: u32 = connection
        .query_row(
            "SELECT version FROM pod0_schema_versions WHERE component='kernel'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read component version", error))?;
    if recorded != version {
        return Err(StorageError::CorruptSchema {
            detail: "component and database versions differ",
        });
    }
    if version >= 2 {
        require_columns(
            connection,
            "pod0_migration_journal",
            &[
                "completed_at_ms",
                "diagnostic_code",
                "from_version",
                "migration_id",
                "started_at_ms",
                "state",
                "to_version",
            ],
        )?;
        require_columns(
            connection,
            "pod0_backup_evidence",
            &[
                "byte_count",
                "created_at_ms",
                "integrity_check",
                "migration_id",
                "page_count",
                "schema_version",
                "store_id",
            ],
        )?;
    }
    if version >= 3 {
        require_columns(
            connection,
            "pod0_domain_cutovers",
            &[
                "committed_at_ms",
                "core_revision",
                "domain",
                "source_generation",
                "state",
            ],
        )?;
    }
    Ok(())
}

fn require_columns(
    connection: &Connection,
    table: &str,
    expected: &[&str],
) -> Result<(), StorageError> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| StorageError::sqlite("inspect table", error))?;
    let actual = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| StorageError::sqlite("inspect table columns", error))?
        .collect::<Result<BTreeSet<_>, _>>()
        .map_err(|error| StorageError::sqlite("read table columns", error))?;
    let expected = expected.iter().map(ToString::to_string).collect();
    if actual == expected {
        Ok(())
    } else {
        Err(StorageError::CorruptSchema {
            detail: "table columns do not match schema",
        })
    }
}

fn table_names(connection: &Connection) -> Result<BTreeSet<String>, StorageError> {
    let mut statement = connection
        .prepare("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%'")
        .map_err(|error| StorageError::sqlite("inspect database tables", error))?;
    statement
        .query_map([], |row| row.get(0))
        .map_err(|error| StorageError::sqlite("query database tables", error))?
        .collect::<Result<_, _>>()
        .map_err(|error| StorageError::sqlite("read database tables", error))
}
