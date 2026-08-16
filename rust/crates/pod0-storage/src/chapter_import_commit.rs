use std::collections::BTreeMap;
use std::path::Path;

use pod0_domain::{ChapterArtifactId, CommandId, EpisodeId};
use rusqlite::params;

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
    crate::transition_commit::commit_chapter_import_cutover(
        source_database_path,
        artifact_root,
        target_path,
        import_id,
        imported_at_ms,
        before_commit,
    )
}

pub(crate) fn apply_verified_chapter_import(
    transaction: &rusqlite::Transaction<'_>,
    import_id: CommandId,
    imported_at_ms: i64,
    report: &ChapterImportReport,
    source: &crate::InspectedChapterSource,
    expected_revision: pod0_domain::StateRevision,
) -> Result<pod0_domain::StateRevision, StorageError> {
    if !matches!(
        report.state,
        ChapterImportState::Verified | ChapterImportState::Imported
    ) || report.plan.blocked_count != 0
    {
        return Err(StorageError::ChapterImportConflict);
    }
    if authority_state(transaction)? != (false, None) {
        return Err(StorageError::ChapterImportConflict);
    }
    if report.state == ChapterImportState::Verified {
        for (episode_id, artifact_id) in selected_artifacts(source)? {
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
    }
    let committed_revision = pod0_domain::StateRevision::new(
        expected_revision
            .value
            .checked_add(1)
            .ok_or(StorageError::ChapterImportConflict)?,
    );
    let state_changed = transaction
        .execute(
            "UPDATE pod0_chapter_state SET collection_revision=MAX(collection_revision,?1) \
             WHERE singleton=1 AND authority_active=0 AND authority_import_id IS NULL",
            [i64::try_from(committed_revision.value)
                .map_err(|_| StorageError::ChapterImportConflict)?],
        )
        .map_err(|error| StorageError::sqlite("advance chapter import revision", error))?;
    if state_changed != 1 {
        return Err(StorageError::ChapterImportConflict);
    }
    if report.state == ChapterImportState::Verified {
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
    }
    let activated = transaction
        .execute(
            "UPDATE pod0_chapter_state SET authority_active=1,authority_import_id=?1 \
             WHERE singleton=1 AND authority_active=0 AND authority_import_id IS NULL",
            [import_id.into_bytes().as_slice()],
        )
        .map_err(|error| StorageError::sqlite("activate chapter authority", error))?;
    if activated != 1 {
        return Err(StorageError::ChapterImportConflict);
    }
    Ok(committed_revision)
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

pub(crate) fn authority_state(
    transaction: &rusqlite::Transaction<'_>,
) -> Result<(bool, Option<[u8; 16]>), StorageError> {
    let value: (bool, Option<Vec<u8>>) = transaction
        .query_row(
            "SELECT authority_active,authority_import_id FROM pod0_chapter_state WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| StorageError::sqlite("read chapter authority before import", error))?;
    Ok((
        value.0,
        value
            .1
            .map(|bytes| {
                bytes
                    .try_into()
                    .map_err(|_| StorageError::ChapterImportConflict)
            })
            .transpose()?,
    ))
}
