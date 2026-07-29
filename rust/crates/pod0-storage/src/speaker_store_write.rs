//! Issue #190: writes for the artifact-external speaker identity.
//!
//! `origin = user` outranks `inferred` permanently: an upsert whose incoming
//! origin is not user never replaces a user-authored row, and completion-time
//! carry-forward always writes `inferred` because diarization indices can
//! permute within a provider across runs.

use pod0_domain::{CommandId, SpeakerEntityId, SpeakerId, TranscriptArtifactId};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::StorageError;
use crate::speaker_store_model::{
    SpeakerAssignmentOrigin, encode_assignment_origin, validate_confidence, validate_display_name,
};
use crate::transcript_authority::require_transcript_authoritative;
use crate::transcript_store::TranscriptStore;

impl TranscriptStore {
    pub fn create_speaker_entity(
        &self,
        speaker_entity_id: SpeakerEntityId,
        display_name: &str,
        created_command_id: Option<CommandId>,
        created_at_ms: i64,
    ) -> Result<(), StorageError> {
        validate_display_name(display_name)?;
        if created_at_ms < 0 {
            return Err(StorageError::InvalidSpeakerEntity);
        }
        self.write(|transaction| {
            require_transcript_authoritative(transaction)?;
            let inserted = transaction
                .execute(
                    "INSERT INTO pod0_speakers(speaker_entity_id,speaker_entity_revision,\
                     display_name,created_at_ms,updated_at_ms,deleted,created_command_id) \
                     VALUES(?1,1,?2,?3,?3,0,?4) \
                     ON CONFLICT(speaker_entity_id) DO NOTHING",
                    params![
                        speaker_entity_id.into_bytes().as_slice(),
                        display_name,
                        created_at_ms,
                        created_command_id.map(|id| id.into_bytes().to_vec()),
                    ],
                )
                .map_err(|error| StorageError::sqlite("create speaker entity", error))?;
            if inserted == 1 {
                Ok(())
            } else {
                Err(StorageError::RevisionConflict)
            }
        })
    }

    pub fn rename_speaker_entity(
        &self,
        speaker_entity_id: SpeakerEntityId,
        display_name: &str,
        renamed_at_ms: i64,
    ) -> Result<(), StorageError> {
        validate_display_name(display_name)?;
        if renamed_at_ms < 0 {
            return Err(StorageError::InvalidSpeakerEntity);
        }
        self.write(|transaction| {
            require_transcript_authoritative(transaction)?;
            let updated = transaction
                .execute(
                    "UPDATE pod0_speakers SET display_name=?2,\
                     speaker_entity_revision=speaker_entity_revision+1,\
                     updated_at_ms=MAX(created_at_ms,?3) \
                     WHERE speaker_entity_id=?1 AND deleted=0",
                    params![
                        speaker_entity_id.into_bytes().as_slice(),
                        display_name,
                        renamed_at_ms,
                    ],
                )
                .map_err(|error| StorageError::sqlite("rename speaker entity", error))?;
            if updated == 1 {
                Ok(())
            } else {
                Err(StorageError::EntityNotFound)
            }
        })
    }

    pub fn assign_speaker(
        &self,
        artifact_id: TranscriptArtifactId,
        speaker_id: SpeakerId,
        speaker_entity_id: SpeakerEntityId,
        origin: SpeakerAssignmentOrigin,
        confidence: Option<f64>,
        decided_at_ms: i64,
    ) -> Result<(), StorageError> {
        validate_confidence(confidence)?;
        if decided_at_ms < 0 {
            return Err(StorageError::InvalidSpeakerEntity);
        }
        self.write(|transaction| {
            require_transcript_authoritative(transaction)?;
            require_active_entity(transaction, speaker_entity_id)?;
            require_artifact_speaker(transaction, artifact_id, speaker_id)?;
            transaction
                .execute(
                    "INSERT INTO pod0_speaker_assignments(artifact_id,speaker_id,\
                     speaker_entity_id,confidence,origin_code,decided_at_ms,decided_command_id) \
                     VALUES(?1,?2,?3,?4,?5,?6,NULL) \
                     ON CONFLICT(artifact_id,speaker_id) DO UPDATE SET \
                     speaker_entity_id=excluded.speaker_entity_id,\
                     confidence=excluded.confidence,origin_code=excluded.origin_code,\
                     decided_at_ms=excluded.decided_at_ms \
                     WHERE pod0_speaker_assignments.origin_code<>1 OR excluded.origin_code=1",
                    params![
                        artifact_id.into_bytes().as_slice(),
                        speaker_id.into_bytes().as_slice(),
                        speaker_entity_id.into_bytes().as_slice(),
                        confidence,
                        encode_assignment_origin(origin),
                        decided_at_ms,
                    ],
                )
                .map_err(|error| StorageError::sqlite("assign speaker entity", error))?;
            Ok(())
        })
    }
}

/// Completion-time carry-forward: when a commit supersedes a previously
/// selected artifact, the prior artifact's assignments are copied onto the
/// new artifact's matching speaker ids as `origin = inferred`. Speakers with
/// no prior assignment (a different provider's disjoint labels, or genuinely
/// new speakers) surface unassigned rather than aliased onto the wrong
/// entity. Existing rows on the new artifact are never overwritten.
pub(crate) fn carry_forward_speaker_assignments(
    transaction: &Transaction<'_>,
    previous_artifact_id: TranscriptArtifactId,
    next_artifact_id: TranscriptArtifactId,
    decided_at_ms: i64,
) -> Result<(), StorageError> {
    transaction
        .execute(
            "INSERT INTO pod0_speaker_assignments(artifact_id,speaker_id,speaker_entity_id,\
             confidence,origin_code,decided_at_ms,decided_command_id) \
             SELECT ?2,prior.speaker_id,prior.speaker_entity_id,prior.confidence,2,?3,NULL \
             FROM pod0_speaker_assignments prior \
             WHERE prior.artifact_id=?1 AND EXISTS(\
                 SELECT 1 FROM pod0_transcript_speakers next \
                 WHERE next.artifact_id=?2 AND next.speaker_id=prior.speaker_id) \
             ON CONFLICT(artifact_id,speaker_id) DO NOTHING",
            params![
                previous_artifact_id.into_bytes().as_slice(),
                next_artifact_id.into_bytes().as_slice(),
                decided_at_ms,
            ],
        )
        .map_err(|error| StorageError::sqlite("carry forward speaker assignments", error))?;
    Ok(())
}

fn require_active_entity(
    connection: &Connection,
    speaker_entity_id: SpeakerEntityId,
) -> Result<(), StorageError> {
    connection
        .query_row(
            "SELECT 1 FROM pod0_speakers WHERE speaker_entity_id=?1 AND deleted=0",
            [speaker_entity_id.into_bytes().as_slice()],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read speaker entity", error))?
        .ok_or(StorageError::EntityNotFound)
}

fn require_artifact_speaker(
    connection: &Connection,
    artifact_id: TranscriptArtifactId,
    speaker_id: SpeakerId,
) -> Result<(), StorageError> {
    connection
        .query_row(
            "SELECT 1 FROM pod0_transcript_speakers WHERE artifact_id=?1 AND speaker_id=?2",
            params![
                artifact_id.into_bytes().as_slice(),
                speaker_id.into_bytes().as_slice(),
            ],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read transcript speaker", error))?
        .ok_or(StorageError::TranscriptNotFound)
}
