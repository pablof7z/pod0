use pod0_application::{
    EvidenceRebuildActivityInput, EvidenceRebuildMutation, evidence_phase_command_id,
    plan_evidence_rebuild,
};
use pod0_domain::{CommandId, ContentDigest, StateRevision, TranscriptEvidenceArtifact};

use super::TransitionCommit;
use crate::evidence_store_read::{read_artifact, selected_generation_id};
use crate::{StorageError, TransitionIngress, TransitionIngressKind};

pub(crate) fn commit_evidence_rebuild(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: ContentDigest,
    artifact: &TranscriptEvidenceArtifact,
    effect: Option<pod0_application::DurableEvidenceEmbeddingEffectRequest>,
    committed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    commit_evidence_rebuild_with(
        path,
        command_id,
        command_fingerprint,
        artifact,
        effect,
        committed_at_ms,
        || Ok(()),
    )
}

fn commit_evidence_rebuild_with<F>(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: ContentDigest,
    artifact: &TranscriptEvidenceArtifact,
    effect: Option<pod0_application::DurableEvidenceEmbeddingEffectRequest>,
    committed_at_ms: i64,
    after_activity: F,
) -> Result<StateRevision, StorageError>
where
    F: FnOnce() -> Result<(), StorageError>,
{
    artifact
        .verify_integrity()
        .map_err(crate::evidence_codec::artifact_error)?;
    let mut after_activity = Some(after_activity);
    let receipt = TransitionCommit::open(path)?.commit_planned_with_transaction_hooks(
        TransitionIngress {
            kind: TransitionIngressKind::ApplicationCommand,
            id: command_id.into_bytes(),
            fingerprint: command_fingerprint,
        },
        pod0_domain::UnixTimestampMilliseconds::new(committed_at_ms.max(0)),
        |transaction| {
            let current_revision = core_revision(transaction)?;
            let selected = selected_generation_id(transaction, artifact.version.episode_id)?;
            if selected == Some(artifact.generation_id)
                && read_artifact(transaction, artifact.generation_id)? != Some(artifact.clone())
            {
                return Err(StorageError::InvalidEvidenceArtifact);
            }
            plan_evidence_rebuild(EvidenceRebuildActivityInput {
                command_id,
                episode_id: artifact.version.episode_id,
                current_revision,
                semantic_change: selected != Some(artifact.generation_id),
                effect: effect.clone(),
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |_| Ok(()),
        |transaction, expected, mutation| {
            require_core_revision(transaction, expected)?;
            match mutation {
                EvidenceRebuildMutation::Apply => {
                    let generation_id = artifact.generation_id;
                    crate::evidence_store_stage::apply_evidence_stage(
                        transaction,
                        evidence_phase_command_id(generation_id, b"stage"),
                        artifact,
                        committed_at_ms,
                    )?;
                    crate::evidence_store_mutations::apply_evidence_verification(
                        transaction,
                        evidence_phase_command_id(generation_id, b"verify"),
                        generation_id,
                        committed_at_ms,
                    )?;
                    crate::evidence_store_mutations::apply_evidence_selection(
                        transaction,
                        evidence_phase_command_id(generation_id, b"select"),
                        artifact.version.episode_id,
                        generation_id,
                        committed_at_ms,
                    )?;
                    crate::library_store::advance_playback_revision(transaction)
                }
                EvidenceRebuildMutation::None => Ok(expected),
            }
        },
        |_| after_activity.take().expect("activity hook consumed once")(),
    )?;
    Ok(receipt.committed_revision)
}

#[cfg(test)]
pub(crate) fn commit_evidence_rebuild_with_observer<F>(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: ContentDigest,
    artifact: &TranscriptEvidenceArtifact,
    effect: Option<pod0_application::DurableEvidenceEmbeddingEffectRequest>,
    committed_at_ms: i64,
    after_activity: F,
) -> Result<StateRevision, StorageError>
where
    F: FnOnce() -> Result<(), StorageError>,
{
    commit_evidence_rebuild_with(
        path,
        command_id,
        command_fingerprint,
        artifact,
        effect,
        committed_at_ms,
        after_activity,
    )
}

fn core_revision(connection: &rusqlite::Connection) -> Result<StateRevision, StorageError> {
    let value: i64 = connection
        .query_row(
            "SELECT state_revision FROM pod0_playback_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read evidence rebuild revision", error))?;
    super::application_support::revision(value)
}

fn require_core_revision(
    transaction: &rusqlite::Transaction<'_>,
    expected: StateRevision,
) -> Result<(), StorageError> {
    (core_revision(transaction)? == expected)
        .then_some(())
        .ok_or(StorageError::RevisionConflict)
}
