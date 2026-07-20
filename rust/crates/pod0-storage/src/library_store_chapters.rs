use pod0_domain::{
    ChapterArtifact, ChapterArtifactId, ChapterArtifactInput, CommandId, ContentDigest, EpisodeId,
    StateRevision,
};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::chapter_authority::require_chapter_authoritative;
use crate::chapter_store_codec::{artifact_id, digest, episode_id, revision};
use crate::chapter_store_read_artifact::read_chapter_artifact;
use crate::chapter_store_write_artifact::insert_or_validate_chapter_artifact;
use crate::transcript_authority::advance_listening_revision;
use crate::{ChapterCommitStorageReceipt, LibraryStore, StorageError};

impl LibraryStore {
    pub fn commit_and_select_chapter(
        &self,
        command_id: CommandId,
        expected_selection_revision: StateRevision,
        input: ChapterArtifactInput,
        completed_at_ms: i64,
    ) -> Result<ChapterCommitStorageReceipt, StorageError> {
        self.commit_and_select_chapter_with_observer(
            command_id,
            expected_selection_revision,
            input,
            completed_at_ms,
            || Ok(()),
        )
    }

    pub(crate) fn commit_and_select_chapter_with_observer<F>(
        &self,
        command_id: CommandId,
        expected_selection_revision: StateRevision,
        input: ChapterArtifactInput,
        completed_at_ms: i64,
        before_commit: F,
    ) -> Result<ChapterCommitStorageReceipt, StorageError>
    where
        F: FnOnce() -> Result<(), StorageError>,
    {
        let artifact =
            ChapterArtifact::seal(input).map_err(|_| StorageError::InvalidChapterArtifact)?;
        if completed_at_ms < 0 || artifact.generated_at.value < 0 {
            return Err(StorageError::InvalidChapterArtifact);
        }
        let fingerprint = artifact.command_fingerprint(expected_selection_revision);
        let resulting_revision = expected_selection_revision
            .value
            .checked_add(1)
            .ok_or(StorageError::ChapterRevisionConflict)?;
        let resulting_revision_i64 =
            i64::try_from(resulting_revision).map_err(|_| StorageError::ChapterRevisionConflict)?;
        let expected_revision_i64 = i64::try_from(expected_selection_revision.value)
            .map_err(|_| StorageError::ChapterRevisionConflict)?;

        self.write(|transaction| {
            require_chapter_authoritative(transaction)?;
            if let Some(receipt) = replay(transaction, command_id, fingerprint, &artifact)? {
                return Ok(receipt);
            }
            require_episode_parent(transaction, &artifact)?;
            require_selected_transcript_provenance(transaction, &artifact)?;
            let current = current_selection(transaction, artifact.episode_id)?;
            let current_revision = current.as_ref().map_or(0, |item| item.1.value);
            if current_revision != expected_selection_revision.value {
                return Err(StorageError::ChapterRevisionConflict);
            }
            insert_or_validate_chapter_artifact(transaction, &artifact, None, completed_at_ms)?;
            let previous_artifact_id = current.map(|item| item.0);
            let already_selected = previous_artifact_id == Some(artifact.artifact_id);
            transaction
                .execute(
                    "INSERT INTO pod0_chapter_selections(episode_id,selection_revision,artifact_id,\
                     source_import_id,selected_at_ms) VALUES(?1,?2,?3,NULL,?4)",
                    params![
                        artifact.episode_id.into_bytes().as_slice(),
                        resulting_revision_i64,
                        artifact.artifact_id.into_bytes().as_slice(),
                        completed_at_ms,
                    ],
                )
                .map_err(|error| StorageError::sqlite("select chapter artifact", error))?;
            advance_collection_revision(transaction)?;
            let _ = advance_listening_revision(transaction)?;
            let receipt = receipt(
                command_id,
                fingerprint,
                previous_artifact_id,
                StateRevision::new(resulting_revision),
                already_selected,
                &artifact,
            )?;
            transaction
                .execute(
                    "INSERT INTO pod0_chapter_commands(command_id,operation_code,\
                     command_fingerprint,episode_id,artifact_id,expected_selection_revision,\
                     previous_artifact_id,resulting_selection_revision,already_selected,\
                     completed_at_ms) VALUES(?1,1,?2,?3,?4,?5,?6,?7,?8,?9)",
                    params![
                        command_id.into_bytes().as_slice(),
                        fingerprint.into_bytes().as_slice(),
                        artifact.episode_id.into_bytes().as_slice(),
                        artifact.artifact_id.into_bytes().as_slice(),
                        expected_revision_i64,
                        previous_artifact_id.map(|id| id.into_bytes().to_vec()),
                        resulting_revision_i64,
                        i64::from(receipt.already_selected),
                        completed_at_ms,
                    ],
                )
                .map_err(|error| StorageError::sqlite("record chapter command", error))?;
            before_commit()?;
            Ok(receipt)
        })
    }
}

fn require_episode_parent(
    transaction: &Transaction<'_>,
    artifact: &ChapterArtifact,
) -> Result<(), StorageError> {
    let parent: Option<Vec<u8>> = transaction
        .query_row(
            "SELECT podcast_id FROM pod0_episodes WHERE episode_id=?1",
            [artifact.episode_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read chapter episode parent", error))?;
    if parent
        .as_deref()
        .map(crate::chapter_store_codec::podcast_id)
        .transpose()?
        == Some(artifact.podcast_id)
    {
        Ok(())
    } else {
        Err(StorageError::InvalidChapterArtifact)
    }
}

fn require_selected_transcript_provenance(
    transaction: &Transaction<'_>,
    artifact: &ChapterArtifact,
) -> Result<(), StorageError> {
    let provenance = &artifact.provenance;
    let (Some(version_id), Some(content_digest)) = (
        provenance.transcript_version_id,
        provenance.transcript_content_digest,
    ) else {
        return Ok(());
    };
    let selected: Option<(Vec<u8>, Vec<u8>)> = transaction
        .query_row(
            "SELECT selection.transcript_version_id,documents.content_digest \
             FROM pod0_transcript_selection selection JOIN pod0_transcript_documents documents \
             ON documents.transcript_version_id=selection.transcript_version_id \
             WHERE selection.episode_id=?1",
            [artifact.episode_id.into_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read chapter transcript provenance", error))?;
    match selected {
        Some((stored_version, stored_digest))
            if crate::chapter_store_codec::transcript_version_id(&stored_version)?
                == version_id
                && digest(&stored_digest)? == content_digest =>
        {
            Ok(())
        }
        _ => Err(StorageError::ChapterRevisionConflict),
    }
}

fn current_selection(
    transaction: &Transaction<'_>,
    requested_episode: EpisodeId,
) -> Result<Option<(ChapterArtifactId, StateRevision)>, StorageError> {
    let row: Option<(Vec<u8>, i64)> = transaction
        .query_row(
            "SELECT artifact_id,selection_revision FROM pod0_chapter_selections \
             WHERE episode_id=?1 ORDER BY selection_revision DESC LIMIT 1",
            [requested_episode.into_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read chapter selection revision", error))?;
    row.map(|row| Ok((artifact_id(&row.0)?, revision(row.1)?)))
        .transpose()
}

fn advance_collection_revision(transaction: &Transaction<'_>) -> Result<(), StorageError> {
    let current: i64 = transaction
        .query_row(
            "SELECT collection_revision FROM pod0_chapter_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read chapter collection revision", error))?;
    let next = current
        .checked_add(1)
        .ok_or(StorageError::ChapterRevisionConflict)?;
    transaction
        .execute(
            "UPDATE pod0_chapter_state SET collection_revision=?1 WHERE singleton=1 \
             AND authority_active=1",
            [next],
        )
        .map_err(|error| StorageError::sqlite("advance chapter collection revision", error))?;
    Ok(())
}

#[allow(clippy::type_complexity)]
fn replay(
    transaction: &Transaction<'_>,
    command_id: CommandId,
    requested_fingerprint: ContentDigest,
    requested_artifact: &ChapterArtifact,
) -> Result<Option<ChapterCommitStorageReceipt>, StorageError> {
    let row = transaction
        .query_row(
            "SELECT operation_code,command_fingerprint,episode_id,artifact_id,\
             expected_selection_revision,previous_artifact_id,resulting_selection_revision,\
             already_selected FROM pod0_chapter_commands WHERE command_id=?1",
            [command_id.into_bytes().as_slice()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Option<Vec<u8>>>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                ))
            },
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read chapter command replay", error))?;
    let Some(row) = row else { return Ok(None) };
    if row.0 != 1 {
        return Err(StorageError::ChapterCommandConflict);
    }
    let fingerprint = digest(&row.1)?;
    let stored_artifact_id = artifact_id(&row.3)?;
    let stored_expected = revision(row.4)?;
    let artifact = read_chapter_artifact(transaction, stored_artifact_id)?
        .ok_or(StorageError::InvalidChapterArtifact)?;
    if episode_id(&row.2)? != artifact.episode_id
        || artifact.command_fingerprint(stored_expected) != fingerprint
    {
        return Err(StorageError::InvalidChapterArtifact);
    }
    if fingerprint != requested_fingerprint || artifact != *requested_artifact {
        return Err(StorageError::ChapterCommandConflict);
    }
    let already_selected = match row.7 {
        0 => false,
        1 => true,
        _ => return Err(StorageError::InvalidChapterArtifact),
    };
    Ok(Some(receipt(
        command_id,
        fingerprint,
        row.5.as_deref().map(artifact_id).transpose()?,
        revision(row.6)?,
        already_selected,
        &artifact,
    )?))
}

fn receipt(
    command_id: CommandId,
    command_fingerprint: ContentDigest,
    previous_artifact_id: Option<ChapterArtifactId>,
    selection_revision: StateRevision,
    already_selected: bool,
    artifact: &ChapterArtifact,
) -> Result<ChapterCommitStorageReceipt, StorageError> {
    Ok(ChapterCommitStorageReceipt {
        command_id,
        artifact_id: artifact.artifact_id,
        content_digest: artifact.content_digest,
        integrity_digest: artifact.integrity_digest,
        command_fingerprint,
        previous_artifact_id,
        selection_revision,
        chapter_count: u32::try_from(artifact.chapters.len())
            .map_err(|_| StorageError::InvalidChapterArtifact)?,
        ad_span_count: u32::try_from(artifact.ad_spans.len())
            .map_err(|_| StorageError::InvalidChapterArtifact)?,
        already_selected,
    })
}
