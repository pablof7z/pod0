use pod0_application::{MAX_PUBLICATION_DETAIL_BYTES, PublicationStatusObservation};
use pod0_domain::{PublicationId, PublicationRecord, StateRevision, UnixTimestampMilliseconds};
use rusqlite::params;
use sha2::{Digest as _, Sha256};

use crate::publication_store_codec::{fact_kind_code, stage_code, stage_from_facts};
use crate::publication_store_read::read_publication;
use crate::{PublicationStore, StorageError};

impl PublicationStore {
    pub fn observe(
        &self,
        publication_id: PublicationId,
        observation: &PublicationStatusObservation,
        updated_at: UnixTimestampMilliseconds,
    ) -> Result<PublicationRecord, StorageError> {
        validate_observation(observation)?;
        let encoded =
            serde_json::to_vec(observation).map_err(|_| StorageError::InvalidPublication)?;
        let digest: [u8; 32] = Sha256::digest(&encoded).into();
        self.write(|transaction| {
            let current = read_publication(transaction, publication_id)?
                .ok_or(StorageError::PublicationNotFound)?;
            let duplicate: bool = transaction
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM pod0_publication_facts \
                     WHERE publication_id=?1 AND fact_digest=?2)",
                    params![publication_id.into_bytes().as_slice(), digest.as_slice()],
                    |row| row.get(0),
                )
                .map_err(|error| StorageError::sqlite("find publication fact", error))?;
            if duplicate {
                return Ok(current);
            }
            let sequence: i64 = transaction
                .query_row(
                    "SELECT COALESCE(MAX(sequence_number),0)+1 FROM pod0_publication_facts \
                     WHERE publication_id=?1",
                    [publication_id.into_bytes().as_slice()],
                    |row| row.get(0),
                )
                .map_err(|error| StorageError::sqlite("allocate publication fact", error))?;
            transaction
                .execute(
                    "INSERT INTO pod0_publication_facts(publication_id,sequence_number,fact_digest,\
                     fact_kind_code,route_id,attempt,event_id_hex,observed_at_ms,detail) \
                     VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                    params![
                        publication_id.into_bytes().as_slice(),
                        sequence,
                        digest.as_slice(),
                        fact_kind_code(observation.kind),
                        observation.route_id.map(|id| id.into_bytes().to_vec()),
                        observation
                            .attempt
                            .map(|attempt| attempt.to_be_bytes().to_vec()),
                        observation.event_id_hex,
                        observation.observed_at.map(|time| time.value),
                        observation.detail,
                    ],
                )
                .map_err(|error| StorageError::sqlite("append publication fact", error))?;
            let facts = read_publication(transaction, publication_id)?
                .ok_or(StorageError::PublicationNotFound)?
                .facts;
            let stage = stage_from_facts(&facts);
            let event_id = facts
                .iter()
                .rev()
                .find_map(|fact| fact.event_id_hex.as_deref());
            let revision = next_revision(current.revision)?;
            transaction
                .execute(
                    "UPDATE pod0_publications SET state_revision=?1,stage_code=?2,\
                     event_id_hex=COALESCE(?3,event_id_hex),updated_at_ms=?4 \
                     WHERE publication_id=?5",
                    params![
                        i64::try_from(revision.value)
                            .map_err(|_| StorageError::InvalidPublication)?,
                        stage_code(stage),
                        event_id,
                        updated_at.value,
                        publication_id.into_bytes().as_slice(),
                    ],
                )
                .map_err(|error| StorageError::sqlite("fold publication fact", error))?;
            read_publication(transaction, publication_id)?.ok_or(StorageError::PublicationNotFound)
        })
    }
}

fn validate_observation(observation: &PublicationStatusObservation) -> Result<(), StorageError> {
    if observation
        .detail
        .as_ref()
        .is_some_and(|detail| detail.len() > MAX_PUBLICATION_DETAIL_BYTES)
        || observation
            .event_id_hex
            .as_ref()
            .is_some_and(|event| !is_lower_hex(event, 64))
    {
        return Err(StorageError::InvalidPublication);
    }
    Ok(())
}

fn next_revision(current: StateRevision) -> Result<StateRevision, StorageError> {
    current
        .value
        .checked_add(1)
        .map(StateRevision::new)
        .ok_or(StorageError::PublicationConflict)
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
