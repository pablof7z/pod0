use pod0_application::{
    ActivityDomain, AgentExecutionActivityInput, AgentExecutionActivityRequest,
    AgentExecutionContinuation, AgentExecutionKind, AgentRecallEffectPhase, AgentToolAction,
    AgentWorkflowAcceptance, DurableAgentRecallEffectRequest, InternalCommandKind, RecallQuery,
    agent_execution_fence_id, agent_tool_policy, plan_agent_execution_with_request,
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
            AgentExecutionContinuation::AgentRecall
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
            let recall = match &proposal.action {
                AgentToolAction::QueryTranscripts { query, scope, limit } => {
                    let configuration = crate::recall_configuration_store::read_configuration(
                        transaction,
                    )?
                    .unwrap_or_default();
                    let phase = AgentRecallEffectPhase::EmbedQuery;
                    let query_id = pod0_domain::RecallQueryId::from_bytes(
                        proposal.proposal_id.into_bytes(),
                    );
                    let deadline_at = pod0_domain::UnixTimestampMilliseconds::new(
                        observed_at.value.saturating_add(30_000),
                    );
                    Some(DurableAgentRecallEffectRequest {
                        turn_id,
                        request_id: pod0_application::agent_recall_request_id(
                            turn_id, query_id, &phase,
                        ),
                        cancellation_id: after.cancellation_id(),
                        issued_revision: after.projection().revision,
                        deadline_at,
                        query: RecallQuery {
                            query_id,
                            text: query.clone(),
                            scope: *scope,
                            limit: *limit,
                        },
                        embedding_provider: configuration.embedding_provider,
                        embedding_model: configuration.embedding_model,
                        reranker: configuration
                            .reranker_provider
                            .zip(configuration.reranker_model),
                        phase,
                    })
                }
                _ => None,
            };
            let capability = if continuation == AgentExecutionContinuation::NativeCapability {
                Some(super::effect_requests::capability_effect_request(
                    &after,
                    CommandId::from_bytes(command.internal_command_id.into_bytes()),
                    None,
                )?)
            } else {
                None
            };
            plan_agent_execution_with_request(AgentExecutionActivityInput {
                internal_command_id: command.internal_command_id,
                authorizing_activity_id: command.authorizing_activity_id,
                correlation_id: command.correlation_id,
                turn_id,
                current_revision: before_projection.revision,
                committed_revision: after.projection().revision,
                continuation,
            }, AgentExecutionActivityRequest { continuation, recall, capability })
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
