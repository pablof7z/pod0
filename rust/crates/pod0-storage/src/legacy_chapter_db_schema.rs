use std::collections::BTreeSet;

use rusqlite::{Connection, OptionalExtension};

use crate::{LegacyChapterSourceKind, StorageError};

const ARTIFACT_COLUMNS_V0: &[&str] = &[
    "content_hash",
    "id",
    "input_version",
    "integrity",
    "kind",
    "location",
    "origin",
    "output_version",
    "schema_version",
    "subject_id",
    "verified_at",
];

pub(crate) fn validate_chapter_source_schema(
    connection: &Connection,
) -> Result<LegacyChapterSourceKind, StorageError> {
    require_columns(
        connection,
        "episodes",
        &["id", "payload", "subscription_id"],
    )?;
    require_columns(connection, "persistence_metadata", &["key", "value"])?;
    if !table_exists(connection, "artifacts")? {
        return Ok(LegacyChapterSourceKind::ArtifactSqliteV1);
    }
    let columns = columns(connection, "artifacts")?;
    let v0 = ARTIFACT_COLUMNS_V0.iter().copied().collect::<BTreeSet<_>>();
    let v1 = v0
        .iter()
        .copied()
        .chain(["selected"])
        .collect::<BTreeSet<_>>();
    let actual = columns.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let kind = if actual == v0 {
        LegacyChapterSourceKind::ArtifactSqliteV0
    } else if actual == v1 {
        LegacyChapterSourceKind::ArtifactSqliteV1
    } else {
        return Err(StorageError::UnsupportedLegacySource);
    };
    let recorded: Option<i64> = if table_exists(connection, "workflow_schema_versions")? {
        connection
            .query_row(
                "SELECT version FROM workflow_schema_versions WHERE component='artifacts'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| StorageError::sqlite("read chapter source schema version", error))?
    } else {
        None
    };
    if recorded.is_some_and(|version| version > 1) {
        return Err(StorageError::NewerLegacyChapterSchema {
            stored: u32::try_from(recorded.unwrap_or(i64::MAX)).unwrap_or(u32::MAX),
            supported: 1,
        });
    }
    match (kind, recorded) {
        (LegacyChapterSourceKind::ArtifactSqliteV0, None | Some(0))
        | (LegacyChapterSourceKind::ArtifactSqliteV1, None | Some(1)) => Ok(kind),
        _ => Err(StorageError::UnsupportedLegacySource),
    }
}

pub(crate) fn source_generation(connection: &Connection) -> Result<u64, StorageError> {
    let value: String = connection
        .query_row(
            "SELECT CAST(value AS TEXT) FROM persistence_metadata WHERE key='generation'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read chapter source generation", error))?;
    value
        .parse()
        .map_err(|_| StorageError::InvalidLegacyRecord {
            entity: "chapter source",
            index: 0,
            detail: "persistence generation is invalid",
        })
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool, StorageError> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?1)",
            [table],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("inspect chapter source table", error))
}

fn require_columns(
    connection: &Connection,
    table: &'static str,
    required: &[&str],
) -> Result<(), StorageError> {
    let actual = columns(connection, table)?;
    if required.iter().all(|column| actual.contains(*column)) {
        Ok(())
    } else {
        Err(StorageError::UnsupportedLegacySource)
    }
}

fn columns(connection: &Connection, table: &str) -> Result<BTreeSet<String>, StorageError> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| StorageError::sqlite("inspect chapter source columns", error))?;
    statement
        .query_map([], |row| row.get(1))
        .map_err(|error| StorageError::sqlite("query chapter source columns", error))?
        .collect::<Result<_, _>>()
        .map_err(|error| StorageError::sqlite("read chapter source columns", error))
}
