use std::collections::BTreeMap;
use std::path::Path;

use pod0_domain::{ChapterArtifactId, CommandId, EpisodeId};
use rusqlite::{TransactionBehavior, params};

use crate::chapter_import_store_read::{open_current, read_import_report};
use crate::chapter_import_verification::mark_corrupt;
use crate::legacy_chapter_source::inspect_chapter_source;
use crate::{ChapterEvidenceValidation, ChapterImportReport, ChapterImportState, StorageError};

pub(crate) fn commit_chapter_import(
    source_database_path: &Path,
    artifact_root: &Path,
    target_path: &Path,
    import_id: CommandId,
    imported_at_ms: i64,
) -> Result<ChapterImportReport, StorageError> {
    commit_chapter_import_with_observer(
        source_database_path,
        artifact_root,
        target_path,
        import_id,
        imported_at_ms,
        || Ok(()),
    )
}

pub(crate) fn commit_chapter_import_with_observer<F>(
    source_database_path: &Path,
    artifact_root: &Path,
    target_path: &Path,
    import_id: CommandId,
    imported_at_ms: i64,
    before_commit: F,
) -> Result<ChapterImportReport, StorageError>
where
    F: FnOnce() -> Result<(), StorageError>,
{
    if imported_at_ms < 0 {
        return Err(StorageError::ChapterImportConflict);
    }
    let source = inspect_chapter_source(source_database_path, artifact_root)?;
    let mut connection = open_current(target_path)?;
    let report = read_import_report(&connection, import_id, true)?
        .ok_or(StorageError::ChapterImportNotFound)?;
    if report.state == ChapterImportState::Imported {
        return Ok(report);
    }
    if report.state != ChapterImportState::Verified || report.plan.blocked_count != 0 {
        return Err(StorageError::ChapterImportConflict);
    }
    if report.plan != source.plan {
        mark_corrupt(target_path, import_id, StorageError::SourceChanged.code());
        return Err(StorageError::SourceChanged);
    }
    let selections = selected_artifacts(&source)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| StorageError::sqlite("begin chapter import commit", error))?;
    require_inactive(&transaction)?;
    for (episode_id, artifact_id) in selections {
        transaction
            .execute(
                "INSERT INTO pod0_chapter_selections(episode_id,selection_revision,artifact_id,\
                 source_import_id,selected_at_ms) VALUES(?1,?2,?3,?4,?5)",
                params![
                    episode_id.into_bytes().as_slice(),
                    i64::try_from(report.target_revision.value)
                        .map_err(|_| StorageError::ChapterImportConflict)?,
                    artifact_id.into_bytes().as_slice(),
                    import_id.into_bytes().as_slice(),
                    imported_at_ms,
                ],
            )
            .map_err(|error| StorageError::sqlite("record chapter import selection", error))?;
    }
    let state_changed = transaction
        .execute(
            "UPDATE pod0_chapter_state SET collection_revision=?1 \
             WHERE singleton=1 AND collection_revision < ?1 AND authority_active=0",
            [i64::try_from(report.target_revision.value)
                .map_err(|_| StorageError::ChapterImportConflict)?],
        )
        .map_err(|error| StorageError::sqlite("advance chapter import revision", error))?;
    if state_changed != 1 {
        return Err(StorageError::ChapterImportConflict);
    }
    let import_changed = transaction
        .execute(
            "UPDATE pod0_chapter_imports SET state='imported',imported_at_ms=?1,\
             diagnostic_code=NULL WHERE import_id=?2 AND state='verified'",
            params![imported_at_ms, import_id.into_bytes().as_slice()],
        )
        .map_err(|error| StorageError::sqlite("commit chapter import", error))?;
    if import_changed != 1 {
        return Err(StorageError::ChapterImportConflict);
    }
    if let Err(error) = before_commit() {
        drop(transaction);
        return Err(error);
    }
    let current = match inspect_chapter_source(source_database_path, artifact_root) {
        Ok(current) => current,
        Err(error) => {
            drop(transaction);
            mark_corrupt(target_path, import_id, error.code());
            return Err(error);
        }
    };
    if current != source {
        drop(transaction);
        mark_corrupt(target_path, import_id, StorageError::SourceChanged.code());
        return Err(StorageError::SourceChanged);
    }
    transaction
        .commit()
        .map_err(|error| StorageError::sqlite("commit chapter import transaction", error))?;
    read_import_report(&connection, import_id, true)?.ok_or(StorageError::ChapterImportNotFound)
}

fn selected_artifacts(
    source: &crate::InspectedChapterSource,
) -> Result<BTreeMap<EpisodeId, ChapterArtifactId>, StorageError> {
    let mut selected = BTreeMap::new();
    for entry in &source.entries {
        if !entry.importer_selected
            || !matches!(
                entry.kind,
                crate::ChapterEvidenceKind::EpisodeAdjunct
                    | crate::ChapterEvidenceKind::WorkflowChapters
            )
        {
            continue;
        }
        if entry.validation != ChapterEvidenceValidation::Canonical {
            return Err(StorageError::InvalidChapterArtifact);
        }
        let artifact = entry
            .artifact
            .as_ref()
            .ok_or(StorageError::InvalidChapterArtifact)?;
        if selected
            .insert(artifact.episode_id, artifact.artifact_id)
            .is_some()
        {
            return Err(StorageError::ChapterImportConflict);
        }
    }
    if selected.len() != source.plan.selected_count as usize {
        return Err(StorageError::ChapterImportConflict);
    }
    Ok(selected)
}

fn require_inactive(transaction: &rusqlite::Transaction<'_>) -> Result<(), StorageError> {
    let value: (bool, Option<Vec<u8>>) = transaction
        .query_row(
            "SELECT authority_active,authority_import_id FROM pod0_chapter_state WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| StorageError::sqlite("read chapter authority before import", error))?;
    if value == (false, None) {
        Ok(())
    } else {
        Err(StorageError::CutoverAlreadyAuthoritative)
    }
}
