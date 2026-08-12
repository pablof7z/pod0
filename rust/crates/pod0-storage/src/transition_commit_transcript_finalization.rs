use pod0_application::{
    TranscriptEvidenceCompletionActivityInput, TranscriptFinalizationActivityInput,
    TranscriptWorkflowActivityIdentity, plan_transcript_evidence_completion,
    plan_transcript_finalization,
};
use pod0_domain::{ContentDigest, StateRevision, UnixTimestampMilliseconds};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::{
    StorageError, StoredTranscriptWorkflowStage, TranscriptWorkflowCommitInput,
    TranscriptWorkflowCommitReceipt, TransitionIngress, TransitionIngressKind,
};

pub(crate) fn commit_transcript_finalization(
    path: &std::path::Path,
    input: TranscriptWorkflowCommitInput,
) -> Result<TranscriptWorkflowCommitReceipt, StorageError> {
    let store = crate::LibraryStore::open_authoritative(path)?;
    let planned = std::cell::RefCell::new(None);
    TransitionCommit::open(path)?.commit_resolved_ingress_with(
        UnixTimestampMilliseconds::new(input.completed_at_ms),
        |transaction| {
            let current = crate::transcript_workflow::read_workflow(transaction, input.episode_id)?
                .ok_or(StorageError::TranscriptWorkflowNotFound)?;
            if current.request_id != Some(input.request_id) {
                return Err(StorageError::StaleTranscriptAttempt);
            }
            let revision = match current.stage {
                StoredTranscriptWorkflowStage::CompletionObserved => current.workflow_revision,
                StoredTranscriptWorkflowStage::EvidenceRequested => StateRevision::new(
                    current
                        .workflow_revision
                        .value
                        .checked_sub(1)
                        .ok_or(StorageError::TranscriptWorkflowConflict)?,
                ),
                _ => return Err(StorageError::StaleTranscriptAttempt),
            };
            let identity = TranscriptWorkflowActivityIdentity::new(
                current.request.workflow_id,
                revision,
                TranscriptWorkflowActivityIdentity::FINALIZATION_PHASE,
            );
            let ingress = TransitionIngress {
                kind: TransitionIngressKind::Recovery,
                id: identity.transaction_id().into_bytes(),
                fingerprint: finalization_fingerprint(&current, &input),
            };
            *planned.borrow_mut() = Some((current, revision));
            Ok(ingress)
        },
        |_| {
            let planned = planned.borrow();
            let (current, revision) = planned.as_ref().ok_or(StorageError::InvalidActivity)?;
            plan_transcript_finalization(TranscriptFinalizationActivityInput {
                command_id: current.command_id,
                episode_id: current.episode_id,
                workflow_id: current.request.workflow_id,
                workflow_revision: *revision,
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, _| {
            let receipt = crate::transcript_workflow::apply_transcript_workflow_commit(
                transaction,
                input.clone(),
            )?;
            let expected_committed = expected
                .value
                .checked_add(1)
                .ok_or(StorageError::InvalidActivity)?;
            if receipt.workflow.workflow_revision.value != expected_committed {
                return Err(StorageError::RevisionConflict);
            }
            Ok(receipt.workflow.workflow_revision)
        },
    )?;
    read_receipt(&store, &input)
}

pub(crate) fn commit_transcript_evidence_completion(
    path: &std::path::Path,
    workflow_id: pod0_domain::TranscriptWorkflowId,
    input_version: &str,
    completed_at_ms: i64,
) -> Result<crate::TranscriptWorkflowRecord, StorageError> {
    let store = crate::LibraryStore::open_authoritative(path)?;
    let planned = std::cell::RefCell::new(None);
    TransitionCommit::open(path)?.commit_resolved_ingress_with(
        UnixTimestampMilliseconds::new(completed_at_ms),
        |transaction| {
            let current = workflow_by_id(transaction, workflow_id)?;
            let revision = match current.stage {
                StoredTranscriptWorkflowStage::EvidenceRequested => current.workflow_revision,
                StoredTranscriptWorkflowStage::Succeeded => StateRevision::new(
                    current
                        .workflow_revision
                        .value
                        .checked_sub(1)
                        .ok_or(StorageError::TranscriptWorkflowConflict)?,
                ),
                _ => return Err(StorageError::TranscriptWorkflowConflict),
            };
            let identity = TranscriptWorkflowActivityIdentity::new(
                workflow_id,
                revision,
                TranscriptWorkflowActivityIdentity::EVIDENCE_COMPLETION_PHASE,
            );
            *planned.borrow_mut() = Some((current, revision));
            Ok(TransitionIngress {
                kind: TransitionIngressKind::Recovery,
                id: identity.transaction_id().into_bytes(),
                fingerprint: evidence_completion_fingerprint(workflow_id, input_version),
            })
        },
        |_| {
            let planned = planned.borrow();
            let (current, revision) = planned.as_ref().ok_or(StorageError::InvalidActivity)?;
            plan_transcript_evidence_completion(TranscriptEvidenceCompletionActivityInput {
                command_id: current.command_id,
                episode_id: current.episode_id,
                workflow_id,
                workflow_revision: *revision,
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, ()| {
            let workflow = crate::transcript_workflow::apply_transcript_evidence_completion(
                transaction,
                workflow_id,
                input_version,
                completed_at_ms,
            )?;
            if workflow.workflow_revision.value != expected.value.saturating_add(1) {
                return Err(StorageError::RevisionConflict);
            }
            Ok(workflow.workflow_revision)
        },
    )?;
    let episode_id = planned
        .borrow()
        .as_ref()
        .ok_or(StorageError::InvalidActivity)?
        .0
        .episode_id;
    store
        .transcript_workflow(episode_id)?
        .ok_or(StorageError::TranscriptWorkflowNotFound)
}

fn workflow_by_id(
    connection: &rusqlite::Connection,
    workflow_id: pod0_domain::TranscriptWorkflowId,
) -> Result<crate::TranscriptWorkflowRecord, StorageError> {
    let episode: Vec<u8> = connection
        .query_row(
            "SELECT episode_id FROM pod0_transcript_workflows WHERE workflow_id=?1",
            [workflow_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(|_| StorageError::TranscriptWorkflowNotFound)?;
    let episode_id = pod0_domain::EpisodeId::from_bytes(
        episode
            .try_into()
            .map_err(|_| StorageError::TranscriptWorkflowConflict)?,
    );
    crate::transcript_workflow::read_workflow(connection, episode_id)?
        .ok_or(StorageError::TranscriptWorkflowNotFound)
}

fn evidence_completion_fingerprint(
    workflow_id: pod0_domain::TranscriptWorkflowId,
    input_version: &str,
) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/transcript/evidence-completion/v1");
    hash.update(workflow_id.into_bytes());
    hash.update(input_version.as_bytes());
    ContentDigest::from_bytes(hash.finalize().into())
}

fn read_receipt(
    store: &crate::LibraryStore,
    input: &TranscriptWorkflowCommitInput,
) -> Result<TranscriptWorkflowCommitReceipt, StorageError> {
    store.read(|connection| {
        let workflow = crate::transcript_workflow::read_workflow(connection, input.episode_id)?
            .ok_or(StorageError::TranscriptWorkflowNotFound)?;
        let artifact_id = workflow
            .committed_artifact_id
            .ok_or(StorageError::TranscriptWorkflowConflict)?;
        let artifact =
            crate::transcript_store_read_artifact::read_artifact_by_id(connection, artifact_id)?
                .ok_or(StorageError::InvalidTranscriptArtifact)?;
        let fingerprint = pod0_domain::transcript_command_fingerprint(
            workflow.expected_selection_revision,
            &artifact,
        );
        let transcript = crate::transcript_store_write::replay_transcript_commit(
            connection,
            workflow.command_id,
            fingerprint,
            &artifact,
        )?
        .ok_or(StorageError::TranscriptWorkflowConflict)?;
        Ok(TranscriptWorkflowCommitReceipt {
            workflow,
            transcript,
        })
    })
}

fn finalization_fingerprint(
    record: &crate::TranscriptWorkflowRecord,
    input: &TranscriptWorkflowCommitInput,
) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/transcript/finalization/v1");
    hash.update(record.episode_id.into_bytes());
    hash.update(record.request.workflow_id.into_bytes());
    hash.update(input.request_id.into_bytes());
    hash.update(input.evidence_input_version.as_bytes());
    hash.update(record.expected_selection_revision.value.to_be_bytes());
    ContentDigest::from_bytes(hash.finalize().into())
}
