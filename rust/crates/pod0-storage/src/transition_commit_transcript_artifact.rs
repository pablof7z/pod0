use pod0_application::{
    RequestDisposition, RequestRejectionReason, TranscriptArtifactActivityInput,
    TranscriptArtifactMutation, plan_transcript_artifact_commit,
};
use pod0_domain::{
    CommandId, ContentDigest, StateRevision, TranscriptArtifact, TranscriptArtifactInput,
    UnixTimestampMilliseconds,
};

use super::TransitionCommit;
use crate::transcript_store_write::{
    commit_and_select_transcript_in_transaction, current_selection, replay_transcript_commit,
};
use crate::{StorageError, TransitionIngress, TransitionIngressKind};

pub(crate) fn commit_transcript_artifact(
    path: &std::path::Path,
    command_id: CommandId,
    activity_fingerprint: ContentDigest,
    expected_selection_revision: StateRevision,
    input: TranscriptArtifactInput,
    completed_at_ms: i64,
) -> Result<crate::TranscriptCommitStorageReceipt, StorageError> {
    let episode_id = input.episode_id;
    let sealed = TranscriptArtifact::seal(input);
    let artifact_is_valid = sealed.is_ok()
        && completed_at_ms >= 0
        && sealed
            .as_ref()
            .is_ok_and(|artifact| artifact.generated_at.value >= 0);
    let store = crate::TranscriptStore::open_authoritative(path)?;
    let artifact = sealed.ok();
    let ingress = TransitionIngress {
        kind: TransitionIngressKind::ApplicationCommand,
        id: command_id.into_bytes(),
        fingerprint: activity_fingerprint,
    };
    let receipt = TransitionCommit::open(path)?
        .commit_planned_with(
            ingress,
            UnixTimestampMilliseconds::new(completed_at_ms.max(0)),
            |transaction| {
                let current = current_selection(transaction, episode_id)?
                    .map_or(StateRevision::INITIAL, |item| item.1);
                let legacy = artifact
                    .as_ref()
                    .map(|artifact| {
                        replay_transcript_commit(
                            transaction,
                            command_id,
                            pod0_domain::transcript_command_fingerprint(
                                expected_selection_revision,
                                artifact,
                            ),
                            artifact,
                        )
                    })
                    .transpose()?
                    .flatten();
                plan_transcript_artifact_commit(TranscriptArtifactActivityInput {
                    command_id,
                    episode_id,
                    current_selection_revision: current,
                    expected_selection_revision,
                    legacy_replay: legacy.is_some(),
                    artifact_is_valid,
                })
                .map(|plan| plan.map_mutation(|mutation| (mutation, legacy)))
                .map_err(|_| StorageError::InvalidActivity)
            },
            |transaction, expected, (mutation, legacy)| match mutation {
                TranscriptArtifactMutation::Commit => {
                    let artifact = artifact
                        .as_ref()
                        .ok_or(StorageError::InvalidTranscriptArtifact)?;
                    Ok(commit_and_select_transcript_in_transaction(
                        transaction,
                        command_id,
                        expected_selection_revision,
                        artifact,
                        completed_at_ms,
                    )?
                    .selection_revision)
                }
                TranscriptArtifactMutation::RecordRejection => {
                    let actual = current_selection(transaction, episode_id)?
                        .map_or(StateRevision::INITIAL, |item| item.1);
                    if actual != expected {
                        return Err(StorageError::RevisionConflict);
                    }
                    Ok(expected)
                }
                TranscriptArtifactMutation::LegacyDuplicate => Ok(legacy
                    .as_ref()
                    .expect("planned legacy transcript replay")
                    .selection_revision),
            },
        )
        .map_err(|error| match error {
            StorageError::ActivityCommandConflict => StorageError::TranscriptCommandConflict,
            other => other,
        })?;
    match receipt.disposition {
        RequestDisposition::Rejected {
            reason: RequestRejectionReason::RevisionConflict,
        } => Err(StorageError::TranscriptRevisionConflict),
        RequestDisposition::Rejected { .. } => Err(StorageError::InvalidTranscriptArtifact),
        RequestDisposition::Accepted | RequestDisposition::Duplicate => {
            let artifact = artifact.ok_or(StorageError::InvalidTranscriptArtifact)?;
            store
                .read(|connection| {
                    replay_transcript_commit(
                        connection,
                        command_id,
                        pod0_domain::transcript_command_fingerprint(
                            expected_selection_revision,
                            &artifact,
                        ),
                        &artifact,
                    )
                })?
                .ok_or(StorageError::TranscriptCommandConflict)
        }
        _ => Err(StorageError::InvalidActivity),
    }
}
