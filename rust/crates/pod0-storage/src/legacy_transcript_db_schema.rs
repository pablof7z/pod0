use std::collections::BTreeSet;

use rusqlite::{Connection, OptionalExtension};

use crate::{LegacyTranscriptSourceKind, StorageError};

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

const ARTIFACT_COLUMNS_V1: &[&str] = &[
    "content_hash",
    "id",
    "input_version",
    "integrity",
    "kind",
    "location",
    "origin",
    "output_version",
    "schema_version",
    "selected",
    "subject_id",
    "verified_at",
];

pub(crate) fn source_kind(
    connection: &Connection,
) -> Result<LegacyTranscriptSourceKind, StorageError> {
    if !table_exists(connection, "artifacts")? {
        return Ok(LegacyTranscriptSourceKind::ArtifactSqliteV1);
    }
    let columns = columns(connection, "artifacts")?;
    let v0 = names(ARTIFACT_COLUMNS_V0);
    let v1 = names(ARTIFACT_COLUMNS_V1);
    if columns == v0 {
        Ok(LegacyTranscriptSourceKind::ArtifactSqliteV0)
    } else if columns == v1 {
        Ok(LegacyTranscriptSourceKind::ArtifactSqliteV1)
    } else {
        Err(StorageError::UnsupportedLegacySource)
    }
}

pub(crate) fn validate_recorded_schema(
    connection: &Connection,
    source_kind: LegacyTranscriptSourceKind,
) -> Result<(), StorageError> {
    if !table_exists(connection, "workflow_schema_versions")? {
        return Ok(());
    }
    let metadata = columns(connection, "workflow_schema_versions")?;
    if metadata != names(&["component", "version"]) {
        return Err(StorageError::UnsupportedLegacySource);
    }
    let version: Option<i64> = connection
        .query_row(
            "SELECT version FROM workflow_schema_versions WHERE component='artifacts'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read legacy transcript schema", error))?;
    let Some(version) = version else {
        return Ok(());
    };
    let version = u32::try_from(version).map_err(|_| StorageError::UnsupportedLegacySource)?;
    if version > 1 {
        return Err(StorageError::NewerLegacyTranscriptSchema {
            stored: version,
            supported: 1,
        });
    }
    if version != source_kind.schema_version() {
        return Err(StorageError::UnsupportedLegacySource);
    }
    Ok(())
}

pub(crate) fn source_generation(connection: &Connection) -> Result<u64, StorageError> {
    if !table_exists(connection, "persistence_metadata")? {
        return Ok(0);
    }
    let value: Option<String> = connection
        .query_row(
            "SELECT CAST(value AS TEXT) FROM persistence_metadata WHERE key='generation'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read transcript source generation", error))?;
    value
        .as_deref()
        .unwrap_or("0")
        .parse()
        .map_err(|_| StorageError::InvalidLegacyRecord {
            entity: "transcript selection",
            index: 0,
            detail: "source generation is invalid",
        })
}

fn columns(connection: &Connection, table: &str) -> Result<BTreeSet<String>, StorageError> {
    let sql = format!("PRAGMA table_info({table})");
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| StorageError::sqlite("inspect legacy transcript columns", error))?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| StorageError::sqlite("read legacy transcript columns", error))?;
    rows.collect::<Result<BTreeSet<_>, _>>()
        .map_err(|error| StorageError::sqlite("decode legacy transcript columns", error))
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool, StorageError> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?1)",
            [table],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("inspect legacy transcript table", error))
}

fn names(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}
