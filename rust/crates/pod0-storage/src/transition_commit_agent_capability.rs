use pod0_application::{
    AgentActionObservation, AgentActionOutcome, AgentCapabilityOutcome,
    AgentEffectObservationActivityInput, AgentPublicationTransition, AgentWorkflowAcceptance,
    DurableAgentCapabilityOutcome, EffectOutcome, agent_host_failure_outcome,
    continuation_model_fence_id, plan_agent_effect_observation,
};
use pod0_domain::{AgentTurnId, CommandId, ContentDigest, StateRevision};
use rusqlite::OptionalExtension;
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use super::agent_capability_generated::{
    generated_audio_action_outcome, generated_audio_input,
};
use crate::agent_store::{persist, read_turn};
use crate::{
    AgentAuditKind, AgentCapabilityObservationCommitInput, AgentCapabilityObservationCommitOutcome,
    AgentCommandContext, EffectOutboxError, StorageError, TransitionIngress, TransitionIngressKind,
};

pub(crate) fn commit_agent_capability_observation(
    path: &std::path::Path,
    input: AgentCapabilityObservationCommitInput,
) -> Result<AgentCapabilityObservationCommitOutcome, StorageError> {
    let store = crate::AgentStore::open(path)?;
    let command_id = CommandId::from_bytes(input.observation.request_id.into_bytes());
    let fingerprint = observation_fingerprint(&input);
    let staged = input.observation.clone();
    let lease = input.lease;
    let planned_turn = std::cell::Cell::new(None);
    let planned_outcome = std::cell::Cell::new(None);
    let receipt = TransitionCommit::open(path)?.commit_planned_with_transaction_hooks(
        TransitionIngress {
            kind: TransitionIngressKind::HostObservation,
            id: lease.attempt_id.into_bytes(),
            fingerprint,
        },
        input.committed_at,
        |transaction| {
            let turn_id = effect_turn(transaction, lease.intent_id)?;
            planned_turn.set(Some(turn_id));
            let before = read_turn(transaction, turn_id)?
                .ok_or(StorageError::AgentTurnNotFound)?;
            validate_request(&before, &input.observation)?;
            let before_revision = before.projection().revision;
            let (mut after, acceptance, effect_outcome) = fold(before, &input.observation)?;
            planned_outcome.set(Some(effect_outcome));
            let replay = acceptance == AgentWorkflowAcceptance::Duplicate;
            if !replay && acceptance != AgentWorkflowAcceptance::Updated {
                return Err(StorageError::AgentTurnConflict);
            }
            if !replay && after.projection().stage
                == pod0_application::AgentTurnStage::Committed
            {
                let projection = after.projection();
                let fence = continuation_model_fence_id(projection.turn_id, projection.revision);
                if after.continue_after_commit(fence, input.observation.observed_at)
                    != AgentWorkflowAcceptance::Updated
                {
                    return Err(StorageError::AgentTurnConflict);
                }
            }
            let committed = if replay {
                StateRevision::new(before_revision.value.checked_add(1)
                    .ok_or(StorageError::InvalidAgentState)?)
            } else {
                after.projection().revision
            };
            let generated = generated_audio_input(&after, &input.observation)?;
            let next_authorization = if !replay
                && after.projection().stage == pod0_application::AgentTurnStage::AwaitingModel
            {
                Some(pod0_application::AgentEffectAuthorization::Model(
                    super::effect_requests::model_effect_request(
                        transaction,
                        &after,
                        command_id,
                        None,
                    )?,
                ))
            } else {
                None
            };
            plan_agent_effect_observation(AgentEffectObservationActivityInput {
                command_id,
                request_id: input.observation.request_id,
                turn_id,
                current_revision: before_revision,
                committed_revision: committed,
                intent_id: lease.intent_id,
                attempt_id: lease.attempt_id,
                authorizing_activity_id: lease.authorizing_activity_id,
                correlation_id: lease.correlation_id,
                episode_id: generated.as_ref().map(|value| value.episode_id),
                outcome: effect_outcome,
                transition: AgentPublicationTransition::ToolStateChanged,
                next_authorization,
                advance_turn: false,
            })
            .map(|plan| plan.map_mutation(|mutation| (mutation, after, replay, generated)))
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction| {
            let turn_id = planned_turn.get().ok_or(StorageError::InvalidActivity)?;
            let effect_outcome = planned_outcome.get().ok_or(StorageError::InvalidActivity)?;
            crate::effect_outbox::stage_agent_capability_observation_in_transaction(
                transaction,
                lease,
                turn_id,
                &staged,
                effect_outcome,
            )
            .map_err(effect_error)
        },
        |transaction, expected, (_, after, replay, generated_audio)| {
            if replay {
                return Err(StorageError::AgentTurnConflict);
            }
            let turn_id = planned_turn.get().ok_or(StorageError::InvalidActivity)?;
            let current =
                read_turn(transaction, turn_id)?.ok_or(StorageError::AgentTurnNotFound)?;
            if current.projection().revision != expected {
                return Err(StorageError::RevisionConflict);
            }
            if let Some(generated_audio) = generated_audio.as_ref() {
                crate::agent_generated_audio_store::commit_generated_audio_artifact_in_transaction(
                    transaction,
                    generated_audio,
                    input.observation.observed_at,
                )?;
            }
            let outcome = persist(
                transaction,
                AgentCommandContext {
                    command_id,
                    command_fingerprint: fingerprint.into_bytes(),
                    observed_at: input.committed_at,
                },
                Some(expected),
                AgentAuditKind::ActionObserved,
                &after,
            )?;
            Ok(outcome.state().projection().revision)
        },
        |transaction| {
            crate::effect_outbox::complete_host_observation_in_transaction(transaction, lease)
                .map_err(effect_error)
        },
    )?;
    let turn_id = match planned_turn.get() {
        Some(value) => value,
        None => store.read(|connection| effect_turn(connection, lease.intent_id))?,
    };
    let state = store
        .turn(turn_id)?
        .ok_or(StorageError::AgentTurnNotFound)?;
    Ok(AgentCapabilityObservationCommitOutcome {
        state,
        replayed: receipt.replayed,
    })
}

fn fold(
    mut state: pod0_application::AgentTurnState,
    observation: &pod0_application::DurableAgentCapabilityHostObservation,
) -> Result<
    (
        pod0_application::AgentTurnState,
        AgentWorkflowAcceptance,
        EffectOutcome,
    ),
    StorageError,
> {
    let projection = state.projection();
    let proposal = projection
        .proposal
        .as_ref()
        .ok_or(StorageError::InvalidAgentState)?;
    let fence = projection
        .execution_fence_id
        .ok_or(StorageError::InvalidAgentState)?;
    let (outcome, effect) = match &observation.outcome {
        DurableAgentCapabilityOutcome::Observed { outcome, .. } => (
            map_outcome(&state, outcome.clone())?,
            capability_effect_outcome(outcome),
        ),
        DurableAgentCapabilityOutcome::Failed { code, safe_detail } => (
            AgentActionOutcome::Failed {
                safe_detail: safe_detail.clone(),
            },
            agent_host_failure_outcome(*code),
        ),
        DurableAgentCapabilityOutcome::Cancelled => {
            (AgentActionOutcome::Cancelled, EffectOutcome::Cancelled)
        }
    };
    let acceptance = state.observe_action(AgentActionObservation {
        proposal_id: proposal.proposal_id,
        execution_fence_id: fence,
        outcome,
        observed_at: observation.observed_at,
    });
    Ok((state, acceptance, effect))
}

fn map_outcome(
    state: &pod0_application::AgentTurnState,
    value: AgentCapabilityOutcome,
) -> Result<AgentActionOutcome, StorageError> {
    Ok(match value {
        AgentCapabilityOutcome::Succeeded { bounded_result } => AgentActionOutcome::Succeeded {
            bounded_result,
            artifact_id: None,
            recall_evidence: Vec::new(),
        },
        AgentCapabilityOutcome::Failed { safe_detail } => {
            AgentActionOutcome::Failed { safe_detail }
        }
        AgentCapabilityOutcome::Cancelled => AgentActionOutcome::Cancelled,
        AgentCapabilityOutcome::OutcomeAmbiguous => AgentActionOutcome::OutcomeAmbiguous,
        AgentCapabilityOutcome::GeneratedAudioStaged { evidence } => {
            generated_audio_action_outcome(state, &evidence)?
        }
    })
}

fn capability_effect_outcome(value: &AgentCapabilityOutcome) -> EffectOutcome {
    match value {
        AgentCapabilityOutcome::Succeeded { .. } => EffectOutcome::Succeeded,
        AgentCapabilityOutcome::Cancelled => EffectOutcome::Cancelled,
        AgentCapabilityOutcome::OutcomeAmbiguous => EffectOutcome::OutcomeUnknown,
        AgentCapabilityOutcome::GeneratedAudioStaged { .. } => EffectOutcome::Succeeded,
        AgentCapabilityOutcome::Failed { .. } => EffectOutcome::Failed {
            code: pod0_application::ActivityFailureCode::PlatformFailure,
        },
    }
}

fn validate_request(
    state: &pod0_application::AgentTurnState,
    observation: &pod0_application::DurableAgentCapabilityHostObservation,
) -> Result<(), StorageError> {
    let projection = state.projection();
    let proposal = projection
        .proposal
        .as_ref()
        .ok_or(StorageError::AgentTurnConflict)?;
    let fence = projection
        .execution_fence_id
        .ok_or(StorageError::AgentTurnConflict)?;
    let expected = pod0_application::agent_capability_request_id(
        projection.turn_id,
        proposal.proposal_id,
        fence,
    );
    let identity_matches = match &observation.outcome {
        DurableAgentCapabilityOutcome::Observed {
            turn_id,
            proposal_id,
            execution_fence_id,
            ..
        } => {
            *turn_id == projection.turn_id
                && *proposal_id == proposal.proposal_id
                && *execution_fence_id == fence
        }
        _ => true,
    };
    if observation.request_id != expected
        || observation.cancellation_id != state.cancellation_id()
        || !identity_matches
    {
        return Err(StorageError::AgentTurnConflict);
    }
    Ok(())
}

fn effect_turn(
    connection: &rusqlite::Connection,
    intent_id: pod0_domain::EffectIntentId,
) -> Result<AgentTurnId, StorageError> {
    let bytes: Option<Vec<u8>> = connection.query_row(
            "SELECT subject_id FROM pod0_effect_intents WHERE intent_id=?1 AND effect_kind_code=10 AND subject_code=4",
            [intent_id.into_bytes().as_slice()], |row| row.get(0),
        ).optional().map_err(|error| StorageError::sqlite("read agent capability effect subject", error))?;
    bytes.map(|value| value.try_into().map(AgentTurnId::from_bytes).map_err(|_| StorageError::InvalidActivity)).transpose()?.ok_or(StorageError::AgentTurnNotFound)
}

include!("transition_commit_agent_capability_support.rs");
