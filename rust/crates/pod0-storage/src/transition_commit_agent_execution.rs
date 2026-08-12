use pod0_application::{
    ActivityDomain, AgentExecutionActivityInput, AgentExecutionContinuation, AgentExecutionKind,
    AgentToolAction, AgentWorkflowAcceptance, InternalCommandKind, agent_execution_fence_id,
    agent_tool_policy, plan_agent_execution,
};
use pod0_domain::{CommandId, ContentDigest};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::agent_store::{persist, read_turn};
use crate::{
    AgentAuditKind, AgentCommandContext, PendingInternalCommand, StorageError, TransitionIngress,
    TransitionIngressKind,
};

pub(crate) fn commit_agent_execution(
    path: &std::path::Path,
    command: PendingInternalCommand,
    observed_at: pod0_domain::UnixTimestampMilliseconds,
) -> Result<pod0_application::AgentTurnState, StorageError> {
    let InternalCommandKind::AdvanceAgentTurn { turn_id } = command.request.kind else {
        return Err(StorageError::InvalidActivity);
    };
    if command.request.target != ActivityDomain::AgentPublication
        || command.request.subject != (pod0_application::ActivitySubject::AgentTurn { turn_id })
        || command.request.episode_id.is_some()
    {
        return Err(StorageError::InvalidActivity);
    }
    let store = crate::AgentStore::open(path)?;
    let fingerprint = fingerprint(command.internal_command_id);
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
            let mut after = before.clone();
            let fence = agent_execution_fence_id(proposal.proposal_id, proposal.proposal_digest);
            if after.begin_execution(fence, observed_at) != AgentWorkflowAcceptance::Updated {
                return Err(StorageError::AgentTurnConflict);
            }
            let execution = agent_tool_policy(proposal.action.tool()).execution;
            let continuation = match execution {
        AgentExecutionKind::NativeCapability
        | AgentExecutionKind::NativeConversationPresentation
        | AgentExecutionKind::NativeCapabilityAndNmpPublication => {
            AgentExecutionContinuation::NativeCapability
        }
        AgentExecutionKind::RustProjection
            if matches!(proposal.action, AgentToolAction::QueryTranscripts { .. }) =>
        {
            AgentExecutionContinuation::None
        }
        AgentExecutionKind::RustProjection => AgentExecutionContinuation::RustProjection,
        AgentExecutionKind::RustCommit
            if matches!(
                proposal.action,
                AgentToolAction::CreateNote { .. }
                    | AgentToolAction::RecordMemory { .. }
                    | AgentToolAction::CreateClip { .. }
                    | AgentToolAction::WriteCategory { .. }
                    | AgentToolAction::TagItems { .. }
            ) =>
        {
            AgentExecutionContinuation::RustTool {
                target: ActivityDomain::UserArtifact,
            }
        }
        AgentExecutionKind::RustCommit => AgentExecutionContinuation::None,
            };
            plan_agent_execution(AgentExecutionActivityInput {
                internal_command_id: command.internal_command_id,
                authorizing_activity_id: command.authorizing_activity_id,
                correlation_id: command.correlation_id,
                turn_id,
                current_revision: before_projection.revision,
                committed_revision: after.projection().revision,
                continuation,
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
                AgentAuditKind::ExecutionStarted,
                &after,
            )?;
            Ok(outcome.state().projection().revision)
        },
    )?;
    store.turn(turn_id)?.ok_or(StorageError::AgentTurnNotFound)
}

fn fingerprint(command_id: pod0_domain::InternalCommandId) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/agent/begin-execution/v2");
    hash.update(command_id.into_bytes());
    ContentDigest::from_bytes(hash.finalize().into())
}
