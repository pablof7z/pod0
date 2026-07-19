use pod0_domain::{CommandId, EpisodeId, EvidenceGenerationId};
use rusqlite::{OptionalExtension, params};

use crate::evidence_commands::{EvidenceOperation, fingerprint, record, replay};
use crate::evidence_store::EvidenceStore;
use crate::evidence_store_read::{read_artifact, read_summary, selected_generation_id};
use crate::{
    EvidenceGenerationState, EvidencePruneReceipt, EvidenceSelectionReceipt,
    EvidenceVerificationReceipt, StorageError,
};

impl EvidenceStore {
    pub fn verify_generation(
        &self,
        command_id: CommandId,
        generation_id: EvidenceGenerationId,
        verified_at_ms: i64,
    ) -> Result<EvidenceVerificationReceipt, StorageError> {
        let command_fingerprint = fingerprint(EvidenceOperation::Verify, generation_id, None, None);
        self.write(|transaction| {
            if let Some(stored) = replay(transaction, command_id, command_fingerprint)? {
                if stored.operation != EvidenceOperation::Verify
                    || stored.generation_id != generation_id
                    || stored.episode_id.is_some()
                {
                    return Err(StorageError::EvidenceCommandConflict);
                }
                return Ok(EvidenceVerificationReceipt {
                    generation_id,
                    already_verified: stored.result,
                });
            }
            read_artifact(transaction, generation_id)?.ok_or(StorageError::EvidenceNotFound)?;
            let summary =
                read_summary(transaction, generation_id)?.ok_or(StorageError::EvidenceNotFound)?;
            let already_verified = summary.state == EvidenceGenerationState::Verified;
            if !already_verified {
                let changed = transaction
                    .execute(
                        "UPDATE pod0_evidence_generations SET state='verified',verified_at_ms=?1 \
                         WHERE generation_id=?2 AND state='staged'",
                        params![verified_at_ms, generation_id.into_bytes().as_slice()],
                    )
                    .map_err(|error| StorageError::sqlite("verify evidence generation", error))?;
                if changed != 1 {
                    return Err(StorageError::InvalidEvidenceArtifact);
                }
            }
            record(
                transaction,
                command_id,
                EvidenceOperation::Verify,
                command_fingerprint,
                generation_id,
                None,
                None,
                already_verified,
                verified_at_ms,
            )?;
            Ok(EvidenceVerificationReceipt {
                generation_id,
                already_verified,
            })
        })
    }

    pub fn select_generation(
        &self,
        command_id: CommandId,
        episode_id: EpisodeId,
        generation_id: EvidenceGenerationId,
        selected_at_ms: i64,
    ) -> Result<EvidenceSelectionReceipt, StorageError> {
        let command_fingerprint = fingerprint(
            EvidenceOperation::Select,
            generation_id,
            Some(episode_id),
            None,
        );
        self.write(|transaction| {
            if let Some(stored) = replay(transaction, command_id, command_fingerprint)? {
                if stored.operation != EvidenceOperation::Select
                    || stored.generation_id != generation_id
                    || stored.episode_id != Some(episode_id)
                {
                    return Err(StorageError::EvidenceCommandConflict);
                }
                return Ok(EvidenceSelectionReceipt {
                    episode_id,
                    generation_id,
                    previous_generation_id: stored.previous_generation_id,
                    already_selected: stored.result,
                });
            }
            let summary = read_summary(transaction, generation_id)?
                .ok_or(StorageError::EvidenceNotFound)?;
            if summary.episode_id != episode_id {
                return Err(StorageError::EvidenceEpisodeMismatch);
            }
            if summary.state != EvidenceGenerationState::Verified {
                return Err(StorageError::EvidenceNotVerified);
            }
            read_artifact(transaction, generation_id)?
                .ok_or(StorageError::EvidenceNotFound)?;
            let previous_generation_id = selected_generation_id(transaction, episode_id)?;
            let already_selected = previous_generation_id == Some(generation_id);
            if !already_selected {
                transaction
                    .execute(
                        "INSERT INTO pod0_evidence_selection(episode_id,generation_id,\
                         generation_state,selected_at_ms) VALUES(?1,?2,'verified',?3) \
                         ON CONFLICT(episode_id) DO UPDATE SET generation_id=excluded.generation_id,\
                         generation_state='verified',selected_at_ms=excluded.selected_at_ms",
                        params![
                            episode_id.into_bytes().as_slice(),
                            generation_id.into_bytes().as_slice(),
                            selected_at_ms,
                        ],
                    )
                    .map_err(|error| StorageError::sqlite("select evidence generation", error))?;
            }
            record(
                transaction,
                command_id,
                EvidenceOperation::Select,
                command_fingerprint,
                generation_id,
                Some(episode_id),
                previous_generation_id,
                already_selected,
                selected_at_ms,
            )?;
            Ok(EvidenceSelectionReceipt {
                episode_id,
                generation_id,
                previous_generation_id,
                already_selected,
            })
        })
    }

    pub fn prune_unselected_generation(
        &self,
        command_id: CommandId,
        generation_id: EvidenceGenerationId,
        completed_at_ms: i64,
    ) -> Result<EvidencePruneReceipt, StorageError> {
        let command_fingerprint = fingerprint(EvidenceOperation::Prune, generation_id, None, None);
        self.write(|transaction| {
            if let Some(stored) = replay(transaction, command_id, command_fingerprint)? {
                if stored.operation != EvidenceOperation::Prune
                    || stored.generation_id != generation_id
                    || stored.episode_id.is_some()
                {
                    return Err(StorageError::EvidenceCommandConflict);
                }
                return Ok(EvidencePruneReceipt {
                    generation_id,
                    pruned: stored.result,
                });
            }
            let selected: Option<i64> = transaction
                .query_row(
                    "SELECT 1 FROM pod0_evidence_selection WHERE generation_id=?1",
                    [generation_id.into_bytes().as_slice()],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|error| StorageError::sqlite("check selected evidence", error))?;
            if selected.is_some() {
                return Err(StorageError::EvidenceGenerationSelected);
            }
            let transcript_version: Option<Vec<u8>> = transaction
                .query_row(
                    "SELECT transcript_version_id FROM pod0_evidence_generations \
                     WHERE generation_id=?1",
                    [generation_id.into_bytes().as_slice()],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|error| StorageError::sqlite("find evidence generation", error))?;
            let pruned = if let Some(transcript_version) = transcript_version {
                transaction
                    .execute(
                        "DELETE FROM pod0_evidence_generations WHERE generation_id=?1",
                        [generation_id.into_bytes().as_slice()],
                    )
                    .map_err(|error| StorageError::sqlite("prune evidence generation", error))?;
                transaction
                    .execute(
                        "DELETE FROM pod0_transcript_documents WHERE transcript_version_id=?1 \
                         AND NOT EXISTS(SELECT 1 FROM pod0_evidence_generations \
                         WHERE transcript_version_id=?1) \
                         AND NOT EXISTS(SELECT 1 FROM pod0_transcript_artifacts \
                         WHERE transcript_version_id=?1)",
                        [transcript_version],
                    )
                    .map_err(|error| StorageError::sqlite("prune transcript document", error))?;
                true
            } else {
                false
            };
            record(
                transaction,
                command_id,
                EvidenceOperation::Prune,
                command_fingerprint,
                generation_id,
                None,
                None,
                pruned,
                completed_at_ms,
            )?;
            Ok(EvidencePruneReceipt {
                generation_id,
                pruned,
            })
        })
    }
}
