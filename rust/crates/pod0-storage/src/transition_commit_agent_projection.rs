use pod0_application::{
    ActivityDomain, AgentActionObservation, AgentActionOutcome, AgentProjectionCompletionActivityInput,
    AgentTurnStage, AgentWorkflowAcceptance, InternalCommandKind, continuation_model_fence_id,
    plan_agent_projection_completion,
};
use pod0_domain::{CommandId, ContentDigest};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::agent_store::{persist, read_turn};
use crate::{
    AgentAuditKind, AgentCommandContext, PendingInternalCommand, StorageError, TransitionIngress,
    TransitionIngressKind,
};

pub(crate) fn commit_agent_projection_result(
    path: &std::path::Path,
    command: PendingInternalCommand,
    result: Result<String, String>,
    observed_at: pod0_domain::UnixTimestampMilliseconds,
) -> Result<pod0_application::AgentTurnState, StorageError> {
    let InternalCommandKind::ExecuteAgentProjection { turn_id } = &command.request.kind else {
        return Err(StorageError::InvalidActivity);
    };
    let turn_id = *turn_id;
    if command.request.target != ActivityDomain::AgentPublication
        || command.request.subject != (pod0_application::ActivitySubject::AgentTurn { turn_id })
        || command.request.episode_id.is_some()
    {
        return Err(StorageError::InvalidActivity);
    }
    let store = crate::AgentStore::open(path)?;
    let fingerprint = fingerprint(command.internal_command_id, &result)?;
    let ingress = TransitionIngress {
            kind: TransitionIngressKind::InternalCommand,
            id: command.internal_command_id.into_bytes(),
            fingerprint,
        };
    TransitionCommit::open(path)?.commit_planned_with(
        ingress,
        observed_at,
        |transaction| {
            let before = read_turn(transaction, turn_id)?
                .ok_or(StorageError::AgentTurnNotFound)?;
            let before_projection = before.projection();
            let proposal = before_projection.proposal.as_ref()
                .ok_or(StorageError::InvalidAgentState)?;
            let fence = before_projection.execution_fence_id
                .ok_or(StorageError::InvalidAgentState)?;
            let mut after = before.clone();
            let outcome = match result {
                Ok(bounded_result) => AgentActionOutcome::Succeeded {
                    bounded_result,
                    artifact_id: None,
                    recall_evidence: Vec::new(),
                },
                Err(safe_detail) => AgentActionOutcome::Failed {
                    safe_detail: Some(safe_detail),
                },
            };
            if after.observe_action(AgentActionObservation {
                proposal_id: proposal.proposal_id,
                execution_fence_id: fence,
                outcome,
                observed_at,
            }) != AgentWorkflowAcceptance::Updated {
                return Err(StorageError::AgentTurnConflict);
            }
            if after.projection().stage == AgentTurnStage::Committed {
                let projection = after.projection();
                let continuation = continuation_model_fence_id(projection.turn_id, projection.revision);
                if after.continue_after_commit(continuation, observed_at)
                    != AgentWorkflowAcceptance::Updated {
                    return Err(StorageError::AgentTurnConflict);
                }
            }
            plan_agent_projection_completion(AgentProjectionCompletionActivityInput {
                internal_command_id: command.internal_command_id,
                authorizing_activity_id: command.authorizing_activity_id,
                correlation_id: command.correlation_id,
                turn_id,
                current_revision: before_projection.revision,
                committed_revision: after.projection().revision,
                authorize_continuation_model: after.projection().stage == AgentTurnStage::AwaitingModel,
            })
            .map(|plan| plan.map_mutation(|mutation| (mutation, after)))
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, (_, after)| {
            let current =
                read_turn(transaction, turn_id)?.ok_or(StorageError::AgentTurnNotFound)?;
            if current.projection().revision != expected {
                return Err(StorageError::RevisionConflict);
            }
            let outcome = persist(
                transaction,
                AgentCommandContext {
                    command_id: CommandId::from_bytes(command.internal_command_id.into_bytes()),
                    command_fingerprint: fingerprint.into_bytes(),
                    observed_at,
                },
                Some(expected),
                AgentAuditKind::ActionObserved,
                &after,
            )?;
            Ok(outcome.state().projection().revision)
        },
    )?;
    store.turn(turn_id)?.ok_or(StorageError::AgentTurnNotFound)
}

fn fingerprint(
    command_id: pod0_domain::InternalCommandId,
    result: &Result<String, String>,
) -> Result<ContentDigest, StorageError> {
    let mut hash = Sha256::new();
    hash.update(b"pod0/agent/projection-result/v2");
    hash.update(command_id.into_bytes());
    hash.update(serde_json::to_vec(result).map_err(|_| StorageError::InvalidActivity)?);
    Ok(ContentDigest::from_bytes(hash.finalize().into()))
}
