use pod0_domain::{
    CommandId, StateRevision, TranscriptArtifact, TranscriptArtifactInput,
    transcript_command_fingerprint,
};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::StorageError;
use crate::transcript_authority::{
    advance_listening_revision, require_transcript_authoritative, set_episode_transcript_available,
    set_transcript_cutover_revision,
};
use crate::transcript_store::TranscriptStore;
use crate::transcript_store_codec::{
    artifact_error, artifact_id, digest, episode_id, optional_artifact_id, revision, version_id,
};
use crate::transcript_store_model::TranscriptCommitStorageReceipt;
use crate::transcript_store_read_artifact::read_artifact_by_id;
use crate::transcript_store_write_rows::{
    ensure_semantic_document, insert_or_validate_artifact, require_episode_parent,
};

impl TranscriptStore {
    pub fn commit_and_select(
        &self,
        command_id: CommandId,
        expected_selection_revision: StateRevision,
        input: TranscriptArtifactInput,
        completed_at_ms: i64,
    ) -> Result<TranscriptCommitStorageReceipt, StorageError> {
        self.commit_and_select_with_observer(
            command_id,
            expected_selection_revision,
            input,
            completed_at_ms,
            || Ok(()),
        )
    }

    pub(crate) fn commit_and_select_with_observer<F>(
        &self,
        command_id: CommandId,
        expected_selection_revision: StateRevision,
        input: TranscriptArtifactInput,
        completed_at_ms: i64,
        before_commit: F,
    ) -> Result<TranscriptCommitStorageReceipt, StorageError>
    where
        F: FnOnce() -> Result<(), StorageError>,
    {
        let artifact = TranscriptArtifact::seal(input).map_err(artifact_error)?;
        if completed_at_ms < 0 || artifact.generated_at.value < 0 {
            return Err(StorageError::InvalidTranscriptArtifact);
        }
        self.write(|transaction| {
            let receipt = commit_and_select_transcript_in_transaction(
                transaction,
                command_id,
                expected_selection_revision,
                &artifact,
                completed_at_ms,
            )?;
            before_commit()?;
            Ok(receipt)
        })
    }
}

pub(crate) fn commit_and_select_transcript_in_transaction(
    transaction: &Transaction<'_>,
    command_id: CommandId,
    expected_selection_revision: StateRevision,
    artifact: &TranscriptArtifact,
    completed_at_ms: i64,
) -> Result<TranscriptCommitStorageReceipt, StorageError> {
    require_transcript_authoritative(transaction)?;
    let fingerprint = transcript_command_fingerprint(expected_selection_revision, artifact);
    if let Some(receipt) = replay(transaction, command_id, fingerprint, artifact)? {
        return Ok(receipt);
    }
    require_episode_parent(transaction, artifact)?;
    let current = current_selection(transaction, artifact.episode_id)?;
    let current_revision = current.as_ref().map_or(0, |item| item.1.value);
    if current_revision != expected_selection_revision.value {
        return Err(StorageError::TranscriptRevisionConflict);
    }
    ensure_semantic_document(transaction, artifact)?;
    insert_or_validate_artifact(transaction, artifact, None, completed_at_ms)?;
    let resulting_revision = expected_selection_revision
        .value
        .checked_add(1)
        .ok_or(StorageError::TranscriptRevisionConflict)?;
    let resulting_revision_i64 =
        i64::try_from(resulting_revision).map_err(|_| StorageError::TranscriptRevisionConflict)?;
    let expected_revision_i64 = i64::try_from(expected_selection_revision.value)
        .map_err(|_| StorageError::TranscriptRevisionConflict)?;
    let previous_artifact_id = current.map(|item| item.0);
    let already_selected = previous_artifact_id == Some(artifact.artifact_id);
    transaction
        .execute(
            "INSERT INTO pod0_transcript_selection(episode_id,artifact_id,transcript_version_id,\
             selection_revision,selected_at_ms,source_import_id) VALUES(?1,?2,?3,?4,?5,NULL) \
             ON CONFLICT(episode_id) DO UPDATE SET artifact_id=excluded.artifact_id,\
             transcript_version_id=excluded.transcript_version_id,\
             selection_revision=excluded.selection_revision,selected_at_ms=excluded.selected_at_ms,\
             source_import_id=NULL",
            params![
                artifact.episode_id.into_bytes().as_slice(),
                artifact.artifact_id.into_bytes().as_slice(),
                artifact.transcript_version_id.into_bytes().as_slice(),
                resulting_revision_i64,
                completed_at_ms,
            ],
        )
        .map_err(|error| StorageError::sqlite("select transcript artifact", error))?;
    advance_collection_revision(transaction)?;
    set_episode_transcript_available(transaction, artifact)?;
    let _ = advance_listening_revision(transaction)?;
    set_transcript_cutover_revision(transaction, resulting_revision)?;
    let receipt = receipt(
        command_id,
        fingerprint,
        previous_artifact_id,
        StateRevision::new(resulting_revision),
        already_selected,
        artifact,
    )?;
    transaction
        .execute(
            "INSERT INTO pod0_transcript_commands(command_id,operation_code,command_fingerprint,\
             episode_id,artifact_id,transcript_version_id,expected_selection_revision,\
             previous_artifact_id,resulting_selection_revision,already_selected,completed_at_ms) \
             VALUES(?1,1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                command_id.into_bytes().as_slice(),
                fingerprint.into_bytes().as_slice(),
                artifact.episode_id.into_bytes().as_slice(),
                artifact.artifact_id.into_bytes().as_slice(),
                artifact.transcript_version_id.into_bytes().as_slice(),
                expected_revision_i64,
                previous_artifact_id.map(|id| id.into_bytes().to_vec()),
                resulting_revision_i64,
                i64::from(receipt.already_selected),
                completed_at_ms,
            ],
        )
        .map_err(|error| StorageError::sqlite("record transcript command", error))?;
    Ok(receipt)
}

fn current_selection(
    transaction: &Transaction<'_>,
    requested_episode: pod0_domain::EpisodeId,
) -> Result<Option<(pod0_domain::TranscriptArtifactId, StateRevision)>, StorageError> {
    let row: Option<(Vec<u8>, i64)> = transaction
        .query_row(
            "SELECT artifact_id,selection_revision FROM pod0_transcript_selection \
             WHERE episode_id=?1",
            [requested_episode.into_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read transcript selection revision", error))?;
    row.map(|row| Ok((artifact_id(&row.0)?, revision(row.1)?)))
        .transpose()
}

fn advance_collection_revision(transaction: &Transaction<'_>) -> Result<(), StorageError> {
    let current: i64 = transaction
        .query_row(
            "SELECT collection_revision FROM pod0_transcript_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read transcript collection revision", error))?;
    let next = current
        .checked_add(1)
        .ok_or(StorageError::TranscriptRevisionConflict)?;
    transaction
        .execute(
            "UPDATE pod0_transcript_state SET collection_revision=?1,source_import_id=NULL \
             WHERE singleton=1",
            [next],
        )
        .map_err(|error| StorageError::sqlite("advance transcript collection revision", error))?;
    Ok(())
}

#[allow(clippy::type_complexity)]
fn replay(
    transaction: &Transaction<'_>,
    command_id: CommandId,
    requested_fingerprint: pod0_domain::ContentDigest,
    requested_artifact: &TranscriptArtifact,
) -> Result<Option<TranscriptCommitStorageReceipt>, StorageError> {
    let row = transaction
        .query_row(
            "SELECT operation_code,command_fingerprint,episode_id,artifact_id,\
             transcript_version_id,expected_selection_revision,previous_artifact_id,\
             resulting_selection_revision,already_selected FROM pod0_transcript_commands \
             WHERE command_id=?1",
            [command_id.into_bytes().as_slice()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, Option<Vec<u8>>>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                ))
            },
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read transcript command replay", error))?;
    let Some(row) = row else { return Ok(None) };
    let fingerprint = digest(&row.1)?;
    let stored_artifact_id = artifact_id(&row.3)?;
    let stored_version_id = version_id(&row.4)?;
    let stored_expected = revision(row.5)?;
    if row.0 != 1 {
        return Err(StorageError::TranscriptCommandConflict);
    }
    let artifact = read_artifact_by_id(transaction, stored_artifact_id)?
        .ok_or(StorageError::InvalidTranscriptArtifact)?;
    if episode_id(&row.2)? != artifact.episode_id
        || stored_version_id != artifact.transcript_version_id
        || transcript_command_fingerprint(stored_expected, &artifact) != fingerprint
    {
        return Err(StorageError::InvalidTranscriptArtifact);
    }
    if fingerprint != requested_fingerprint || artifact != *requested_artifact {
        return Err(StorageError::TranscriptCommandConflict);
    }
    let already_selected = match row.8 {
        0 => false,
        1 => true,
        _ => return Err(StorageError::InvalidTranscriptArtifact),
    };
    Ok(Some(receipt(
        command_id,
        fingerprint,
        optional_artifact_id(row.6)?,
        revision(row.7)?,
        already_selected,
        &artifact,
    )?))
}

fn receipt(
    command_id: CommandId,
    command_fingerprint: pod0_domain::ContentDigest,
    previous_artifact_id: Option<pod0_domain::TranscriptArtifactId>,
    selection_revision: StateRevision,
    already_selected: bool,
    artifact: &TranscriptArtifact,
) -> Result<TranscriptCommitStorageReceipt, StorageError> {
    Ok(TranscriptCommitStorageReceipt {
        command_id,
        artifact_id: artifact.artifact_id,
        transcript_version_id: artifact.transcript_version_id,
        transcript_content_digest: artifact.content_digest,
        artifact_integrity_digest: artifact.integrity_digest,
        command_fingerprint,
        previous_artifact_id,
        selection_revision,
        speaker_count: u32::try_from(artifact.speakers.len())
            .map_err(|_| StorageError::InvalidTranscriptArtifact)?,
        segment_count: u32::try_from(artifact.segments.len())
            .map_err(|_| StorageError::InvalidTranscriptArtifact)?,
        word_count: artifact
            .segments
            .iter()
            .map(|segment| segment.words.len() as u64)
            .sum(),
        already_selected,
    })
}
