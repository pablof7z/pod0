use pod0_domain::{CommandId, ContentDigest, EpisodeId, EvidenceGenerationId};
use rusqlite::{OptionalExtension, Transaction, params};
use sha2::{Digest as _, Sha256};

use crate::StorageError;
use crate::evidence_codec::{episode_id, generation_id};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EvidenceOperation {
    Stage = 1,
    Verify = 2,
    Select = 3,
    Prune = 4,
}

impl EvidenceOperation {
    fn from_code(value: i64) -> Result<Self, StorageError> {
        match value {
            1 => Ok(Self::Stage),
            2 => Ok(Self::Verify),
            3 => Ok(Self::Select),
            4 => Ok(Self::Prune),
            _ => Err(StorageError::InvalidEvidenceArtifact),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct StoredEvidenceCommand {
    pub operation: EvidenceOperation,
    pub generation_id: EvidenceGenerationId,
    pub episode_id: Option<EpisodeId>,
    pub previous_generation_id: Option<EvidenceGenerationId>,
    pub result: bool,
}

pub(crate) fn fingerprint(
    operation: EvidenceOperation,
    generation_id: EvidenceGenerationId,
    episode_id: Option<EpisodeId>,
    artifact_digest: Option<ContentDigest>,
) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"pod0.evidence-command.v1");
    hash.update([operation as u8]);
    hash.update(generation_id.into_bytes());
    hash_optional(&mut hash, episode_id.map(EpisodeId::into_bytes));
    hash_optional(&mut hash, artifact_digest.map(ContentDigest::into_bytes));
    hash.finalize().into()
}

pub(crate) fn replay(
    transaction: &Transaction<'_>,
    command_id: CommandId,
    expected_fingerprint: [u8; 32],
) -> Result<Option<StoredEvidenceCommand>, StorageError> {
    let row = transaction
        .query_row(
            "SELECT operation_code,command_fingerprint,generation_id,episode_id,\
             previous_generation_id,result_code FROM pod0_evidence_commands WHERE command_id=?1",
            [command_id.into_bytes().as_slice()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Option<Vec<u8>>>(3)?,
                    row.get::<_, Option<Vec<u8>>>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            },
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read evidence command receipt", error))?;
    let Some((operation, stored_fingerprint, generation, episode, previous, result)) = row else {
        return Ok(None);
    };
    if stored_fingerprint.as_slice() != expected_fingerprint {
        return Err(StorageError::EvidenceCommandConflict);
    }
    Ok(Some(StoredEvidenceCommand {
        operation: EvidenceOperation::from_code(operation)?,
        generation_id: generation_id(&generation)?,
        episode_id: episode.as_deref().map(episode_id).transpose()?,
        previous_generation_id: previous.as_deref().map(generation_id).transpose()?,
        result: match result {
            0 => false,
            1 => true,
            _ => return Err(StorageError::InvalidEvidenceArtifact),
        },
    }))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn record(
    transaction: &Transaction<'_>,
    command_id: CommandId,
    operation: EvidenceOperation,
    command_fingerprint: [u8; 32],
    generation_id: EvidenceGenerationId,
    episode_id: Option<EpisodeId>,
    previous_generation_id: Option<EvidenceGenerationId>,
    result: bool,
    completed_at_ms: i64,
) -> Result<(), StorageError> {
    transaction
        .execute(
            "INSERT INTO pod0_evidence_commands(command_id,operation_code,command_fingerprint,\
             generation_id,episode_id,previous_generation_id,result_code,completed_at_ms) \
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                command_id.into_bytes().as_slice(),
                operation as i64,
                command_fingerprint.as_slice(),
                generation_id.into_bytes().as_slice(),
                episode_id.map(EpisodeId::into_bytes),
                previous_generation_id.map(EvidenceGenerationId::into_bytes),
                i64::from(result),
                completed_at_ms,
            ],
        )
        .map_err(|error| StorageError::sqlite("record evidence command receipt", error))?;
    Ok(())
}

fn hash_optional<const N: usize>(hash: &mut Sha256, value: Option<[u8; N]>) {
    match value {
        Some(value) => {
            hash.update([1]);
            hash.update(value);
        }
        None => hash.update([0]),
    }
}
