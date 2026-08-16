use pod0_application::{
    AgentActionObservation, AgentActionOutcome, AgentPublicationTransition,
    AgentRecallHostOutcome, AgentRecallProgressActivityInput, AgentTurnStage,
    AgentWorkflowAcceptance, DurableEffectExecution, DurableExternalEffectRequest, EffectOutcome,
    plan_agent_effect_observation, plan_agent_recall_progress,
};
use pod0_domain::{CommandId, ContentDigest};
use rusqlite::{OptionalExtension, params};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::agent_store::{persist, read_turn};
use crate::{
    AgentAuditKind, AgentCommandContext, AgentRecallObservationCommitInput,
    AgentRecallObservationCommitOutcome, AgentRecallResolution, StorageError, TransitionIngress,
    TransitionIngressKind,
};

pub(crate) fn commit_agent_recall_observation(
    path: &std::path::Path,
    input: AgentRecallObservationCommitInput,
) -> Result<AgentRecallObservationCommitOutcome, StorageError> {
    let request = crate::EffectOutbox::open(path)
        .map_err(|_| StorageError::InvalidActivity)?
        .effect_request(input.lease.intent_id)
        .map_err(|_| StorageError::InvalidActivity)?
        .and_then(|value| match value.execution {
            DurableEffectExecution::AgentRecall { request } => Some(request),
            _ => None,
        })
        .ok_or(StorageError::InvalidActivity)?;
    let fingerprint = fingerprint(&input)?;
    let receipt = TransitionCommit::open(path)?.commit_planned_with_transaction_hooks(
        TransitionIngress {
            kind: TransitionIngressKind::HostObservation,
            id: input.observation.request_id.into_bytes(),
            fingerprint,
        },
        input.committed_at,
        |transaction| {
            let durable = validate(transaction, &input)?;
            let before = read_turn(transaction, durable.turn_id)?
                .ok_or(StorageError::AgentTurnNotFound)?;
            match &input.resolution {
                AgentRecallResolution::Rerank { request: next } => {
                    validate_next(&durable, next, &input)?;
                    plan_agent_recall_progress(AgentRecallProgressActivityInput {
                        command_id: CommandId::from_bytes(input.observation.request_id.into_bytes()),
                        request_id: input.observation.request_id,
                        turn_id: durable.turn_id,
                        current_revision: before.projection().revision,
                        intent_id: input.lease.intent_id,
                        attempt_id: input.lease.attempt_id,
                        authorizing_activity_id: input.lease.authorizing_activity_id,
                        correlation_id: input.lease.correlation_id,
                        next_request: next.clone(),
                    })
                    .map(|plan| plan.map_mutation(|_| None))
                    .map_err(|_| StorageError::InvalidActivity)
                }
                AgentRecallResolution::Finish { .. }
                | AgentRecallResolution::Fail { .. }
                | AgentRecallResolution::Cancelled => {
                    let next = finish_state(before.clone(), &input)?;
                    let projection = next.projection();
                    plan_agent_effect_observation(pod0_application::AgentEffectObservationActivityInput {
                        command_id: CommandId::from_bytes(input.observation.request_id.into_bytes()),
                        request_id: input.observation.request_id,
                        turn_id: durable.turn_id,
                        current_revision: before.projection().revision,
                        committed_revision: projection.revision,
                        intent_id: input.lease.intent_id,
                        attempt_id: input.lease.attempt_id,
                        authorizing_activity_id: input.lease.authorizing_activity_id,
                        correlation_id: input.lease.correlation_id,
                        episode_id: None,
                        outcome: observation_outcome(&input.observation.outcome),
                        transition: AgentPublicationTransition::ToolStateChanged,
                        next_authorization: if projection.stage == AgentTurnStage::AwaitingModel {
                            Some(pod0_application::AgentEffectAuthorization::Model(
                                super::effect_requests::model_effect_request(
                                    transaction,
                                    &next,
                                    CommandId::from_bytes(
                                        input.observation.request_id.into_bytes(),
                                    ),
                                    None,
                                )?,
                            ))
                        } else {
                            None
                        },
                        advance_turn: false,
                    })
                    .map(|plan| plan.map_mutation(|_| Some(next)))
                    .map_err(|_| StorageError::InvalidActivity)
                }
            }
        },
        |transaction| stage(transaction, &input, fingerprint),
        |transaction, expected, planned| {
            let Some(next) = planned else { return Ok(expected) };
            let outcome = persist(
                transaction,
                AgentCommandContext {
                    command_id: CommandId::from_bytes(input.observation.request_id.into_bytes()),
                    command_fingerprint: fingerprint.into_bytes(),
                    observed_at: input.committed_at,
                },
                Some(expected),
                AgentAuditKind::ActionObserved,
                &next,
            )?;
            Ok(outcome.state().projection().revision)
        },
        |transaction| {
            crate::effect_outbox::complete_host_observation_in_transaction(
                transaction,
                input.lease,
            )
            .map_err(|_| StorageError::AgentTurnConflict)
        },
    )?;
    let state = crate::AgentStore::open(path)?
        .turn(request.turn_id)?
        .ok_or(StorageError::AgentTurnNotFound)?;
    Ok(AgentRecallObservationCommitOutcome {
        state,
        replayed: receipt.replayed,
        continued: matches!(input.resolution, AgentRecallResolution::Rerank { .. }),
    })
}

fn validate(
    transaction: &rusqlite::Transaction<'_>,
    input: &AgentRecallObservationCommitInput,
) -> Result<pod0_application::DurableAgentRecallEffectRequest, StorageError> {
    let payload: Option<String> = transaction.query_row(
        "SELECT i.request_json FROM pod0_effect_attempts a JOIN pod0_effect_intents i \
         ON i.intent_id=a.intent_id WHERE a.lease_id=?1 AND a.attempt_id=?2 AND a.intent_id=?3 \
         AND a.fence=?4 AND a.state_code=1 AND a.lease_expires_at_ms>=?5 \
         AND a.lease_expires_at_ms=?6 AND i.authorizing_activity_id=?7 AND i.correlation_id=?8 \
         AND i.effect_kind_code=3 AND i.subject_code=4",
        params![
            input.lease.lease_id.into_bytes().as_slice(),
            input.lease.attempt_id.into_bytes().as_slice(),
            input.lease.intent_id.into_bytes().as_slice(),
            i64::try_from(input.lease.fence).map_err(|_| StorageError::InvalidActivity)?,
            input.observation.observed_at.value,
            input.lease.expires_at.value,
            input.lease.authorizing_activity_id.into_bytes().as_slice(),
            input.lease.correlation_id.into_bytes().as_slice(),
        ],
        |row| row.get(0),
    ).optional().map_err(|error| StorageError::sqlite("validate agent recall lease", error))?;
    let request: DurableExternalEffectRequest = serde_json::from_str(
        payload.as_deref().ok_or(StorageError::AgentTurnConflict)?,
    ).map_err(|_| StorageError::InvalidActivity)?;
    let DurableEffectExecution::AgentRecall { request } = request.execution else {
        return Err(StorageError::AgentTurnConflict);
    };
    if request.request_id != input.observation.request_id
        || request.cancellation_id != input.observation.cancellation_id
        || request.issued_revision != input.observation.observed_request_revision
        || !outcome_matches(&request, &input.observation.outcome)
    {
        return Err(StorageError::AgentTurnConflict);
    }
    Ok(request)
}

fn outcome_matches(
    request: &pod0_application::DurableAgentRecallEffectRequest,
    outcome: &AgentRecallHostOutcome,
) -> bool {
    match (outcome, &request.phase) {
        (AgentRecallHostOutcome::QueryEmbedded { query_id, .. },
            pod0_application::AgentRecallEffectPhase::EmbedQuery)
        | (AgentRecallHostOutcome::CandidatesReranked { query_id, .. },
            pod0_application::AgentRecallEffectPhase::Rerank { .. }) => {
                *query_id == request.query.query_id
            }
        (AgentRecallHostOutcome::Failed { .. }, _)
        | (AgentRecallHostOutcome::Cancelled, _) => true,
        _ => false,
    }
}

fn validate_next(
    prior: &pod0_application::DurableAgentRecallEffectRequest,
    next: &pod0_application::DurableAgentRecallEffectRequest,
    input: &AgentRecallObservationCommitInput,
) -> Result<(), StorageError> {
    if prior.turn_id != next.turn_id
        || prior.query != next.query
        || prior.cancellation_id != next.cancellation_id
        || prior.issued_revision != next.issued_revision
        || prior.embedding_provider != next.embedding_provider
        || prior.embedding_model != next.embedding_model
        || prior.reranker != next.reranker
        || next.deadline_at.value < input.observation.observed_at.value
        || next.request_id
            != pod0_application::agent_recall_request_id(
                next.turn_id,
                next.query.query_id,
                &next.phase,
            )
        || !matches!(prior.phase, pod0_application::AgentRecallEffectPhase::EmbedQuery)
        || !matches!(next.phase, pod0_application::AgentRecallEffectPhase::Rerank { .. })
        || !matches!(input.observation.outcome, AgentRecallHostOutcome::QueryEmbedded { .. })
    {
        return Err(StorageError::AgentTurnConflict);
    }
    let pod0_application::AgentRecallEffectPhase::Rerank { candidates, evidence } = &next.phase
    else {
        return Err(StorageError::AgentTurnConflict);
    };
    if candidates.is_empty()
        || candidates.len() != evidence.len()
        || !candidates.iter().zip(evidence).all(|(candidate, item)| {
            candidate.span_id == item.span_id && candidate.excerpt == item.excerpt
        })
    {
        return Err(StorageError::AgentTurnConflict);
    }
    Ok(())
}

fn finish_state(
    mut state: pod0_application::AgentTurnState,
    input: &AgentRecallObservationCommitInput,
) -> Result<pod0_application::AgentTurnState, StorageError> {
    let before = state.projection();
    let proposal = before.proposal.as_ref().ok_or(StorageError::InvalidAgentState)?;
    let fence = before.execution_fence_id.ok_or(StorageError::InvalidAgentState)?;
    let outcome = match &input.resolution {
        AgentRecallResolution::Finish { bounded_result, evidence } => AgentActionOutcome::Succeeded {
            bounded_result: bounded_result.clone(),
            artifact_id: None,
            recall_evidence: evidence.clone(),
        },
        AgentRecallResolution::Fail { safe_detail } => AgentActionOutcome::Failed {
            safe_detail: safe_detail.clone(),
        },
        AgentRecallResolution::Cancelled => AgentActionOutcome::Cancelled,
        AgentRecallResolution::Rerank { .. } => return Err(StorageError::InvalidAgentState),
    };
    if state.observe_action(AgentActionObservation {
        proposal_id: proposal.proposal_id,
        execution_fence_id: fence,
        outcome,
        observed_at: input.observation.observed_at,
    }) != AgentWorkflowAcceptance::Updated {
        return Err(StorageError::AgentTurnConflict);
    }
    if state.projection().stage == AgentTurnStage::Committed {
        let projection = state.projection();
        let continuation = pod0_application::continuation_model_fence_id(
            projection.turn_id,
            projection.revision,
        );
        if state.continue_after_commit(continuation, input.observation.observed_at)
            != AgentWorkflowAcceptance::Updated
        {
            return Err(StorageError::AgentTurnConflict);
        }
    }
    Ok(state)
}

fn observation_outcome(outcome: &AgentRecallHostOutcome) -> EffectOutcome {
    match outcome {
        AgentRecallHostOutcome::QueryEmbedded { .. }
        | AgentRecallHostOutcome::CandidatesReranked { .. } => EffectOutcome::Succeeded,
        AgentRecallHostOutcome::Cancelled => EffectOutcome::Cancelled,
        AgentRecallHostOutcome::Failed { code, .. } => pod0_application::agent_host_failure_outcome(*code),
    }
}

include!("transition_commit_agent_recall_stage.rs");
