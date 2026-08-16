use std::path::Path;

use pod0_domain::{CommandId, StateRevision};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::StorageError;
use crate::transcript_authority::{
    advance_listening_revision, synchronize_episode_transcript_readiness,
};
use crate::transcript_import_model::{
    StoredTranscriptImportEntry, TranscriptImportReport, TranscriptImportState,
};
use crate::transcript_store_codec::artifact_id;
use crate::transcript_store_read_artifact::read_artifact_by_id;

pub(crate) fn commit_transcript_import(
    source_database_path: &Path,
    transcript_root: &Path,
    target_path: &Path,
    import_id: CommandId,
    committed_at_ms: i64,
) -> Result<TranscriptImportReport, StorageError> {
    crate::transition_commit::commit_transcript_import_cutover(
        source_database_path,
        transcript_root,
        target_path,
        import_id,
        committed_at_ms,
        || Ok(()),
    )
}

#[cfg(test)]
pub(crate) fn commit_transcript_import_with_observer<F>(
    source_database_path: &Path,
    transcript_root: &Path,
    target_path: &Path,
    import_id: CommandId,
    committed_at_ms: i64,
    before_final_source_check: F,
) -> Result<TranscriptImportReport, StorageError>
where
    F: FnOnce() -> Result<(), StorageError>,
{
    crate::transition_commit::commit_transcript_import_cutover(
        source_database_path,
        transcript_root,
        target_path,
        import_id,
        committed_at_ms,
        before_final_source_check,
    )
}

pub(crate) fn apply_verified_transcript_import(
    transaction: &Transaction<'_>,
    import_id: CommandId,
    committed_at_ms: i64,
    current: &TranscriptImportReport,
    entries: &[StoredTranscriptImportEntry],
) -> Result<StateRevision, StorageError> {
    if current.state != TranscriptImportState::Verified {
        return Err(StorageError::TranscriptImportConflict);
    }
    require_reserved_revision(transaction, current.target_revision.value)?;
    for entry in entries.iter().filter(|entry| entry.is_selected) {
        commit_selection(transaction, import_id, entry, committed_at_ms)?;
    }
    transaction
        .execute(
            "DELETE FROM pod0_transcript_selection WHERE source_import_id IS NULL \
             OR source_import_id<>?1",
            [import_id.into_bytes().as_slice()],
        )
        .map_err(|error| {
            StorageError::sqlite("remove pre-authority transcript selections", error)
        })?;
    synchronize_episode_transcript_readiness(transaction)?;
    let committed_revision = StateRevision::new(advance_listening_revision(transaction)?);
    transaction
        .execute(
            "UPDATE pod0_transcript_state SET collection_revision=?1,source_import_id=?2 \
             WHERE singleton=1",
            params![
                to_i64(current.target_revision.value)?,
                import_id.into_bytes().as_slice()
            ],
        )
        .map_err(|error| StorageError::sqlite("commit transcript collection revision", error))?;
    let changed = transaction
        .execute(
            "UPDATE pod0_transcript_imports SET state='committed',committed_at_ms=?1,\
             diagnostic_code=NULL WHERE import_id=?2 AND state='verified'",
            params![committed_at_ms, import_id.into_bytes().as_slice()],
        )
        .map_err(|error| StorageError::sqlite("commit transcript import state", error))?;
    if changed != 1 {
        return Err(StorageError::TranscriptImportConflict);
    }
    let marker_changed = transaction
        .execute(
            "UPDATE pod0_domain_cutovers SET state='authoritative',committed_at_ms=?1 \
             WHERE domain='transcripts' \
             AND state='staged' AND source_generation=?2 AND core_revision=?3",
            params![
                committed_at_ms,
                to_i64(current.plan.source_generation)?,
                to_i64(current.target_revision.value)?,
            ],
        )
        .map_err(|error| StorageError::sqlite("confirm staged transcript marker", error))?;
    if marker_changed != 1 {
        return Err(StorageError::TranscriptImportConflict);
    }
    Ok(committed_revision)
}

fn commit_selection(
    transaction: &Transaction<'_>,
    import_id: CommandId,
    entry: &StoredTranscriptImportEntry,
    selected_at_ms: i64,
) -> Result<(), StorageError> {
    let artifact = read_artifact_by_id(transaction, entry.artifact_id)?
        .ok_or(StorageError::InvalidTranscriptArtifact)?;
    if artifact.episode_id != entry.episode_id
        || artifact.transcript_version_id != entry.transcript_version_id
    {
        return Err(StorageError::InvalidTranscriptArtifact);
    }
    let selected: Option<(Vec<u8>, i64, Option<Vec<u8>>)> = transaction
        .query_row(
            "SELECT artifact_id,selection_revision,source_import_id \
             FROM pod0_transcript_selection WHERE episode_id=?1",
            [entry.episode_id.into_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read imported transcript selection", error))?;
    let selection_revision = match selected {
        None => 1_i64,
        Some((selected_id, revision, _)) if artifact_id(&selected_id)? == entry.artifact_id => {
            revision
        }
        Some((_, revision, _)) if cutover_is_staged(transaction)? => revision
            .checked_add(1)
            .ok_or(StorageError::TranscriptImportConflict)?,
        Some(_) => return Err(StorageError::TranscriptImportConflict),
    };
    transaction
        .execute(
            "INSERT INTO pod0_transcript_selection(episode_id,artifact_id,transcript_version_id,\
             selection_revision,selected_at_ms,source_import_id) VALUES(?1,?2,?3,?4,?5,?6) \
             ON CONFLICT(episode_id) DO UPDATE SET artifact_id=excluded.artifact_id,\
             transcript_version_id=excluded.transcript_version_id,\
             selection_revision=excluded.selection_revision,\
             source_import_id=excluded.source_import_id,selected_at_ms=excluded.selected_at_ms",
            params![
                entry.episode_id.into_bytes().as_slice(),
                entry.artifact_id.into_bytes().as_slice(),
                entry.transcript_version_id.into_bytes().as_slice(),
                selection_revision,
                selected_at_ms,
                import_id.into_bytes().as_slice(),
            ],
        )
        .map_err(|error| StorageError::sqlite("commit imported transcript selection", error))?;
    Ok(())
}

fn cutover_is_staged(transaction: &Transaction<'_>) -> Result<bool, StorageError> {
    let state: Option<String> = transaction
        .query_row(
            "SELECT state FROM pod0_domain_cutovers WHERE domain='transcripts'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read transcript cutover state", error))?;
    Ok(state.as_deref() == Some("staged"))
}

fn require_reserved_revision(
    transaction: &Transaction<'_>,
    target: u64,
) -> Result<(), StorageError> {
    let current: i64 = transaction
        .query_row(
            "SELECT collection_revision FROM pod0_transcript_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read transcript collection revision", error))?;
    let expected = target
        .checked_sub(1)
        .ok_or(StorageError::TranscriptImportConflict)?;
    if u64::try_from(current).ok() == Some(expected) {
        Ok(())
    } else {
        Err(StorageError::TranscriptImportConflict)
    }
}

fn to_i64(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::TranscriptImportConflict)
}
