use pod0_domain::{
    CommandId, EpisodeRecord, PodcastRecord, PublicationId, PublicationIntent, PublicationRecord,
    StateRevision, UnixTimestampMilliseconds,
};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::publication_store_codec::{artifact_kind_code, stage_code};
use crate::publication_store_read::read_publication;
use crate::{PublicationPrepareOutcome, PublicationStore, StorageError};

impl PublicationStore {
    pub fn prepare_generated_episode(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        intent: &PublicationIntent,
        episode: &EpisodeRecord,
        podcast: &PodcastRecord,
        prepared_at: UnixTimestampMilliseconds,
    ) -> Result<PublicationPrepareOutcome, StorageError> {
        crate::transition_commit::commit_publication_prepare(
            &self.path,
            command_id,
            command_fingerprint,
            intent,
            episode,
            podcast,
            prepared_at,
        )
    }

    pub fn record_leased_receipt(
        &self,
        input: pod0_application::LeasedNMPPublicationReceipt,
        observed_at: UnixTimestampMilliseconds,
    ) -> Result<PublicationRecord, StorageError> {
        crate::transition_commit::commit_publication_receipt(&self.path, input, observed_at)
    }
}

pub(crate) fn record_receipt_in_transaction(
    transaction: &Transaction<'_>,
    publication_id: PublicationId,
    receipt_id: u64,
    observed_at: UnixTimestampMilliseconds,
) -> Result<PublicationRecord, StorageError> {
    let current =
        read_publication(transaction, publication_id)?.ok_or(StorageError::PublicationNotFound)?;
    if current
        .receipt_id
        .is_some_and(|stored| stored != receipt_id)
    {
        return Err(StorageError::PublicationConflict);
    }
    if current.receipt_id == Some(receipt_id) {
        return Ok(current);
    }
    let revision = next_revision(current.revision)?;
    transaction
        .execute(
            "UPDATE pod0_publications SET receipt_id=?1,state_revision=?2,updated_at_ms=?3 \
             WHERE publication_id=?4",
            params![
                receipt_id.to_be_bytes().as_slice(),
                i64::try_from(revision.value).map_err(|_| StorageError::InvalidPublication)?,
                observed_at.value,
                publication_id.into_bytes().as_slice(),
            ],
        )
        .map_err(|error| StorageError::sqlite("record publication receipt", error))?;
    read_publication(transaction, publication_id)?.ok_or(StorageError::PublicationNotFound)
}

pub(crate) fn insert_publication(
    transaction: &Transaction<'_>,
    record: &PublicationRecord,
) -> Result<(), StorageError> {
    let (kind, wire) = artifact_kind_code(record.artifact_kind);
    transaction
        .execute(
            "INSERT INTO pod0_publications(publication_id,artifact_id,artifact_kind_code,\
             artifact_kind_wire_code,episode_id,podcast_id,semantic_revision,state_revision,\
             expected_author_hex,correlation_token,public_media_url,media_type,media_byte_count,\
             media_content_digest,receipt_id,event_id_hex,stage_code,prepared_at_ms,updated_at_ms) \
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,NULL,NULL,?15,?16,?17)",
            params![
                record.publication_id.into_bytes().as_slice(),
                record.artifact_id.into_bytes().as_slice(),
                kind,
                wire,
                record.episode_id.into_bytes().as_slice(),
                record.podcast_id.into_bytes().as_slice(),
                i64::from(record.semantic_revision),
                i64::try_from(record.revision.value)
                    .map_err(|_| StorageError::InvalidPublication)?,
                record.expected_author_hex,
                record.correlation_token,
                record.media.public_url,
                record.media.media_type,
                i64::try_from(record.media.byte_count)
                    .map_err(|_| StorageError::InvalidPublication)?,
                record.media.content_digest.into_bytes().as_slice(),
                stage_code(record.stage),
                record.prepared_at.value,
                record.updated_at.value,
            ],
        )
        .map_err(|error| StorageError::sqlite("prepare publication", error))?;
    Ok(())
}

pub(crate) fn command_receipt(
    transaction: &Transaction<'_>,
    command_id: CommandId,
    fingerprint: &str,
) -> Result<Option<PublicationRecord>, StorageError> {
    let row: Option<(String, Vec<u8>)> = transaction
        .query_row(
            "SELECT command_fingerprint,publication_id FROM pod0_publication_commands \
             WHERE command_id=?1",
            [command_id.into_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read publication command", error))?;
    match row {
        None => Ok(None),
        Some((stored, id)) if stored == fingerprint => {
            let id = PublicationId::from_bytes(
                id.try_into()
                    .map_err(|_| StorageError::PublicationCommandConflict)?,
            );
            read_publication(transaction, id)
        }
        Some(_) => Err(StorageError::PublicationCommandConflict),
    }
}

pub(crate) fn insert_command(
    transaction: &Transaction<'_>,
    command_id: CommandId,
    fingerprint: &str,
    publication_id: PublicationId,
    observed_at: UnixTimestampMilliseconds,
) -> Result<(), StorageError> {
    transaction
        .execute(
            "INSERT INTO pod0_publication_commands(command_id,command_fingerprint,publication_id,\
             completed_at_ms) VALUES(?1,?2,?3,?4)",
            params![
                command_id.into_bytes().as_slice(),
                fingerprint,
                publication_id.into_bytes().as_slice(),
                observed_at.value,
            ],
        )
        .map_err(|error| StorageError::sqlite("record publication command", error))?;
    Ok(())
}

pub(crate) fn same_semantics(left: &PublicationRecord, right: &PublicationRecord) -> bool {
    left.publication_id == right.publication_id
        && left.artifact_id == right.artifact_id
        && left.artifact_kind == right.artifact_kind
        && left.episode_id == right.episode_id
        && left.podcast_id == right.podcast_id
        && left.semantic_revision == right.semantic_revision
        && left.expected_author_hex == right.expected_author_hex
        && left.correlation_token == right.correlation_token
        && left.media == right.media
}

fn next_revision(current: StateRevision) -> Result<StateRevision, StorageError> {
    current
        .value
        .checked_add(1)
        .map(StateRevision::new)
        .ok_or(StorageError::PublicationConflict)
}
