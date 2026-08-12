use pod0_application::{
    TranscriptObservationActivityInput, plan_transcript_observation,
    transcript_observation_semantics,
};
use pod0_domain::ContentDigest;
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::{
    EffectOutboxError, StorageError, TranscriptObservationCommitInput,
    TranscriptObservationCommitOutcome, TransitionIngress, TransitionIngressKind,
};

pub(crate) fn commit_transcript_observation(
    path: &std::path::Path,
    input: TranscriptObservationCommitInput,
) -> Result<TranscriptObservationCommitOutcome, StorageError> {
    let store = crate::LibraryStore::open_authoritative(path)?;
    let current = workflow_for_observation(&store, &input)?;
    let (outcome, transition) = transcript_observation_semantics(&input.observation.observation);
    let plan = plan_transcript_observation(TranscriptObservationActivityInput {
        command_id: current.command_id,
        request_id: input.observation.request_id,
        episode_id: current.episode_id,
        workflow_id: current.request.workflow_id,
        workflow_revision: current.workflow_revision,
        intent_id: input.lease.intent_id,
        attempt_id: input.lease.attempt_id,
        authorizing_activity_id: input.lease.authorizing_activity_id,
        correlation_id: input.lease.correlation_id,
        outcome,
        transition,
    })
    .map_err(|_| StorageError::InvalidActivity)?;
    let staged_observation = input.observation.clone();
    let mutation_observation = input.observation.clone();
    let decision = input.decision.clone();
    let receipt = TransitionCommit::open(path)?.commit_with_transaction_hooks(
        TransitionIngress {
            kind: TransitionIngressKind::HostObservation,
            id: input.lease.attempt_id.into_bytes(),
            fingerprint: observation_fingerprint(&input),
        },
        plan,
        input.committed_at,
        |transaction| {
            crate::effect_outbox::stage_host_observation_in_transaction(
                transaction,
                input.lease,
                &staged_observation,
                outcome,
            )
            .map_err(effect_error)
        },
        |transaction, expected, _| {
            let before =
                crate::transcript_workflow::read_workflow(transaction, current.episode_id)?
                    .ok_or(StorageError::TranscriptWorkflowNotFound)?;
            if before.workflow_revision != expected {
                return Err(StorageError::RevisionConflict);
            }
            let updated = super::transcript_observation_apply::apply_observation(
                transaction,
                before,
                mutation_observation,
                decision,
                input.committed_at.value,
            )?;
            Ok(updated.workflow_revision)
        },
        |transaction| {
            crate::effect_outbox::complete_host_observation_in_transaction(transaction, input.lease)
                .map_err(effect_error)
        },
    )?;
    let workflow = store
        .transcript_workflow(current.episode_id)?
        .ok_or(StorageError::TranscriptWorkflowNotFound)?;
    Ok(TranscriptObservationCommitOutcome {
        workflow,
        replayed: receipt.replayed,
    })
}

fn workflow_for_observation(
    store: &crate::LibraryStore,
    input: &TranscriptObservationCommitInput,
) -> Result<crate::TranscriptWorkflowRecord, StorageError> {
    let workflow = store
        .transcript_workflow_for_effect_intent(input.lease.intent_id)?
        .ok_or(StorageError::TranscriptWorkflowNotFound)?;
    if workflow.request_id != Some(input.observation.request_id) {
        return Err(StorageError::StaleTranscriptAttempt);
    }
    Ok(workflow)
}

fn observation_fingerprint(input: &TranscriptObservationCommitInput) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/transcript/effect-observation/v1");
    hash.update(input.lease.intent_id.into_bytes());
    hash.update(input.lease.attempt_id.into_bytes());
    hash.update(input.lease.lease_id.into_bytes());
    hash.update(input.lease.fence.to_be_bytes());
    hash.update(serde_json::to_vec(&input.observation).expect("typed durable observation"));
    ContentDigest::from_bytes(hash.finalize().into())
}

fn effect_error(error: EffectOutboxError) -> StorageError {
    match error {
        EffectOutboxError::StaleLease => StorageError::StaleTranscriptAttempt,
        EffectOutboxError::InvalidRecord | EffectOutboxError::InvalidLeaseDuration => {
            StorageError::InvalidActivity
        }
        EffectOutboxError::Storage => StorageError::Sqlite {
            operation: "commit transcript effect observation",
        },
    }
}
