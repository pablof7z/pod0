use std::path::Path;

use rusqlite::{Connection, TransactionBehavior, params};

use crate::backup::verify_connection;
use crate::chapter_rollback_format::ChapterRollbackManifest;
use crate::{LegacyChapterSourceKind, StorageError};

pub(crate) fn make_replayable_database(
    path: &Path,
    manifest: &ChapterRollbackManifest,
) -> Result<(), StorageError> {
    let mut connection = Connection::open(path)
        .map_err(|error| StorageError::sqlite("open chapter rollback database", error))?;
    verify_connection(&connection)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| StorageError::sqlite("begin chapter rollback database", error))?;
    transaction
        .execute_batch(
            "CREATE TABLE pod0_chapter_rollback(
               singleton INTEGER PRIMARY KEY CHECK(singleton=1),
               format_version INTEGER NOT NULL CHECK(format_version=1)
             ) STRICT;
             INSERT INTO pod0_chapter_rollback VALUES(1,1);",
        )
        .map_err(|error| StorageError::sqlite("mark chapter rollback database", error))?;
    for entry in &manifest.entries {
        if !matches!(
            entry.evidence_kind.as_str(),
            "workflow_chapters" | "workflow_ad_spans"
        ) {
            continue;
        }
        let row_id = entry.source_row_id.ok_or(StorageError::BackupConflict)?;
        let changed = transaction
            .execute(
                "UPDATE artifacts SET location=?1 WHERE id=?2",
                params![
                    entry.relative_path,
                    i64::try_from(row_id).map_err(|_| StorageError::BackupConflict)?,
                ],
            )
            .map_err(|error| StorageError::sqlite("rewrite chapter rollback location", error))?;
        if changed != 1 {
            return Err(StorageError::BackupConflict);
        }
    }
    transaction
        .commit()
        .map_err(|error| StorageError::sqlite("commit chapter rollback database", error))?;
    verify_connection(&connection).map(|_| ())
}

pub(crate) fn verify_replayable_database(
    bundle: &Path,
    manifest: &ChapterRollbackManifest,
) -> Result<(), StorageError> {
    let database = bundle.join(&manifest.database_path);
    let connection = Connection::open(&database)
        .map_err(|error| StorageError::sqlite("open exported chapter rollback", error))?;
    verify_connection(&connection)?;
    let marker: i64 = connection
        .query_row(
            "SELECT format_version FROM pod0_chapter_rollback WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("verify chapter rollback marker", error))?;
    if marker != 1 {
        return Err(StorageError::BackupConflict);
    }
    drop(connection);
    let plan = crate::inspect_legacy_chapter_source(&database, bundle)?;
    let expected_kind = match manifest.source_kind.as_str() {
        "artifact_sqlite_v0" => LegacyChapterSourceKind::ArtifactSqliteV0,
        "artifact_sqlite_v1" => LegacyChapterSourceKind::ArtifactSqliteV1,
        _ => return Err(StorageError::BackupConflict),
    };
    let database_evidence = manifest
        .entries
        .iter()
        .filter(|entry| {
            matches!(
                entry.evidence_kind.as_str(),
                "episode_adjunct" | "workflow_chapters" | "workflow_ad_spans"
            )
        })
        .count();
    let database_blocked = manifest
        .entries
        .iter()
        .filter(|entry| {
            entry.validation_state == "blocked"
                && matches!(
                    entry.evidence_kind.as_str(),
                    "episode_adjunct" | "workflow_chapters" | "workflow_ad_spans"
                )
        })
        .count();
    if plan.source_kind != expected_kind
        || plan.source_generation != manifest.source_generation
        || plan.evidence_count as usize != database_evidence
        || plan.canonical_artifact_count != manifest.artifact_count
        || plan.selected_count != manifest.selected_count
        || plan.blocked_count as usize != database_blocked
    {
        return Err(StorageError::BackupConflict);
    }
    Ok(())
}
