use pod0_application::{
    ActivityDomain, AgentActionObservation, AgentActionOutcome, AgentProjectionCompletionActivityInput,
    AgentToolAction, AgentToolCompletion, AgentTurnStage, AgentWorkflowAcceptance,
    InternalCommandKind, continuation_model_fence_id, plan_agent_projection_completion,
};
use pod0_domain::{CommandId, ContentDigest};
use serde_json::json;
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::agent_store::{persist, read_turn};
use crate::{
    AgentAuditKind, AgentCommandContext, PendingInternalCommand, StorageError, TransitionIngress,
    TransitionIngressKind,
};

pub(crate) fn commit_agent_tool_completion(
    path: &std::path::Path,
    command: PendingInternalCommand,
    observed_at: pod0_domain::UnixTimestampMilliseconds,
) -> Result<pod0_application::AgentTurnState, StorageError> {
    let InternalCommandKind::CompleteAgentTool {
        turn_id,
        completion,
    } = &command.request.kind
    else {
        return Err(StorageError::InvalidActivity);
    };
    let turn_id = *turn_id;
    if command.request.target != ActivityDomain::AgentPublication
        || command.request.subject != (pod0_application::ActivitySubject::AgentTurn { turn_id })
    {
        return Err(StorageError::InvalidActivity);
    }
    let store = crate::AgentStore::open(path)?;
    let fingerprint = fingerprint(command.internal_command_id, *completion);
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
            let outcome = completion_outcome(&proposal.action, *completion)?;
            let mut after = before.clone();
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
            let continuation_model = if after.projection().stage == AgentTurnStage::AwaitingModel {
                Some(super::effect_requests::model_effect_request(
                    transaction,
                    &after,
                    CommandId::from_bytes(command.internal_command_id.into_bytes()),
                    None,
                )?)
            } else {
                None
            };
            plan_agent_projection_completion(AgentProjectionCompletionActivityInput {
                internal_command_id: command.internal_command_id,
                authorizing_activity_id: command.authorizing_activity_id,
                correlation_id: command.correlation_id,
                turn_id,
                current_revision: before_projection.revision,
                committed_revision: after.projection().revision,
                continuation_model,
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

fn completion_outcome(
    action: &AgentToolAction,
    completion: AgentToolCompletion,
) -> Result<AgentActionOutcome, StorageError> {
    match (action, completion) {
        (AgentToolAction::CreateNote { .. }, AgentToolCompletion::NoteCreated { note_id }) => {
            Ok(AgentActionOutcome::Succeeded {
                bounded_result: json!({
                    "note_id": opaque_id(note_id.into_bytes()),
                    "saved": true,
                })
                .to_string(),
                artifact_id: None,
                recall_evidence: Vec::new(),
            })
        }
        (
            AgentToolAction::RecordMemory { .. },
            AgentToolCompletion::MemoryRecorded { memory_id },
        ) => Ok(AgentActionOutcome::Succeeded {
            bounded_result: json!({
                "memory_id": opaque_id(memory_id.into_bytes()),
                "saved": true,
            })
            .to_string(),
            artifact_id: None,
            recall_evidence: Vec::new(),
        }),
        (
            AgentToolAction::CreateClip { .. },
            AgentToolCompletion::ClipCreated { clip_id },
        ) => Ok(AgentActionOutcome::Succeeded {
            bounded_result: json!({
                "clip_id": opaque_id(clip_id.into_bytes()),
                "saved": true,
            })
            .to_string(),
            artifact_id: None,
            recall_evidence: Vec::new(),
        }),
        (
            AgentToolAction::WriteCategory { .. } | AgentToolAction::TagItems { .. },
            AgentToolCompletion::CategoryChanged { category_id },
        ) => Ok(AgentActionOutcome::Succeeded {
            bounded_result: json!({
                "category_id": opaque_id(category_id.into_bytes()),
                "saved": true,
            })
            .to_string(),
            artifact_id: None,
            recall_evidence: Vec::new(),
        }),
        (_, AgentToolCompletion::Failed { code }) => Ok(AgentActionOutcome::Failed {
            safe_detail: Some(format!("agent_tool_failed_{code}")),
        }),
        _ => Err(StorageError::InvalidActivity),
    }
}

fn fingerprint(
    command_id: pod0_domain::InternalCommandId,
    completion: AgentToolCompletion,
) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/agent/tool-completion/v1");
    hash.update(command_id.into_bytes());
    hash.update(serde_json::to_vec(&completion).expect("typed completion"));
    ContentDigest::from_bytes(hash.finalize().into())
}

fn opaque_id(bytes: [u8; 16]) -> String {
    let hex = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}
