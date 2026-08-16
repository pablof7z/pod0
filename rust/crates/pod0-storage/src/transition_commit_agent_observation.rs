use pod0_application::{
    AgentEffectObservationActivityInput, AgentModelObservation, AgentPublicationTransition,
    AgentWorkflowAcceptance, DurableAgentModelOutcome, EffectOutcome, agent_host_failure_outcome,
    parse_agent_tool_call, plan_agent_effect_observation,
};
use pod0_domain::{AgentTurnId, CommandId, ContentDigest, StateRevision};
use rusqlite::OptionalExtension;
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::agent_store::{persist, read_turn};
use crate::{
    AgentAuditKind, AgentCommandContext, AgentModelObservationCommitInput,
    AgentModelObservationCommitOutcome, EffectOutboxError, StorageError, TransitionIngress,
    TransitionIngressKind,
};

pub(crate) fn commit_agent_model_observation(
    path: &std::path::Path,
    input: AgentModelObservationCommitInput,
) -> Result<AgentModelObservationCommitOutcome, StorageError> {
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
            if before_revision.value == u64::MAX {
                return Err(StorageError::InvalidAgentState);
            }
            let (after, acceptance, effect_outcome) = fold(before, &input.observation)?;
            planned_outcome.set(Some(effect_outcome));
            let replay = acceptance == AgentWorkflowAcceptance::Duplicate;
            if !replay && acceptance != AgentWorkflowAcceptance::Updated
                && after.projection().revision.value <= before_revision.value
            {
                return Err(StorageError::AgentTurnConflict);
            }
            let next_authorization = if !replay
                && after.projection().stage == pod0_application::AgentTurnStage::ApprovalRequired
            {
                Some(pod0_application::AgentEffectAuthorization::Approval(
                    super::effect_requests::approval_effect_request(
                        &after,
                        command_id,
                        None,
                    )?,
                ))
            } else {
                None
            };
            let committed = if replay {
                StateRevision::new(before_revision.value + 1)
            } else {
                after.projection().revision
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
                episode_id: None,
                outcome: effect_outcome,
                transition: AgentPublicationTransition::TurnStateChanged,
                next_authorization,
                advance_turn: !replay && after.projection().stage
                    == pod0_application::AgentTurnStage::Authorized,
            })
            .map(|plan| plan.map_mutation(|mutation| (mutation, after, replay)))
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction| {
            let turn_id = planned_turn.get().ok_or(StorageError::InvalidActivity)?;
            let effect_outcome = planned_outcome.get().ok_or(StorageError::InvalidActivity)?;
            crate::effect_outbox::stage_agent_model_observation_in_transaction(
                transaction,
                lease,
                turn_id,
                &staged,
                effect_outcome,
            )
            .map_err(effect_error)
        },
        |transaction, expected, (_, after, replay)| {
            if replay {
                return Err(StorageError::AgentTurnConflict);
            }
            let turn_id = planned_turn.get().ok_or(StorageError::InvalidActivity)?;
            let current =
                read_turn(transaction, turn_id)?.ok_or(StorageError::AgentTurnNotFound)?;
            if current.projection().revision != expected {
                return Err(StorageError::RevisionConflict);
            }
            let outcome = persist(
                transaction,
                AgentCommandContext {
                    command_id,
                    command_fingerprint: fingerprint.into_bytes(),
                    observed_at: input.committed_at,
                },
                Some(expected),
                AgentAuditKind::ModelObserved,
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
    Ok(AgentModelObservationCommitOutcome {
        state,
        replayed: receipt.replayed,
    })
}

fn fold(
    mut state: pod0_application::AgentTurnState,
    observation: &pod0_application::DurableAgentModelHostObservation,
) -> Result<
    (
        pod0_application::AgentTurnState,
        AgentWorkflowAcceptance,
        EffectOutcome,
    ),
    StorageError,
> {
    let (acceptance, outcome) = match &observation.outcome {
        DurableAgentModelOutcome::Completed {
            turn_id,
            model_fence_id,
            assistant_text,
            proposed_tool_call,
            usage,
        } => {
            let proposed_action = match proposed_tool_call {
                Some(call) => match parse_agent_tool_call(call) {
                    Ok(action) => Some(action),
                    Err(_) => {
                        let accepted = state.fail_model(
                            Some("invalid_tool_action".into()),
                            observation.observed_at,
                        );
                        return Ok((state, accepted, EffectOutcome::Succeeded));
                    }
                },
                None => None,
            };
            (
                state.observe_model(AgentModelObservation {
                    turn_id: *turn_id,
                    model_fence_id: *model_fence_id,
                    assistant_text: assistant_text.clone(),
                    proposed_action,
                    usage: *usage,
                    observed_at: observation.observed_at,
                }),
                EffectOutcome::Succeeded,
            )
        }
        DurableAgentModelOutcome::Failed { code, safe_detail } => (
            state.fail_model(safe_detail.clone(), observation.observed_at),
            agent_host_failure_outcome(*code),
        ),
        DurableAgentModelOutcome::Cancelled => (
            state.cancel(observation.observed_at),
            EffectOutcome::Cancelled,
        ),
    };
    Ok((state, acceptance, outcome))
}

fn validate_request(
    state: &pod0_application::AgentTurnState,
    observation: &pod0_application::DurableAgentModelHostObservation,
) -> Result<(), StorageError> {
    let projection = state.projection();
    let expected = pod0_application::agent_model_request_id(
        projection.turn_id,
        projection
            .execution_fence_id
            .ok_or(StorageError::AgentTurnConflict)?,
    );
    if observation.request_id != expected || observation.cancellation_id != state.cancellation_id()
    {
        return Err(StorageError::AgentTurnConflict);
    }
    Ok(())
}

fn effect_turn(
    connection: &rusqlite::Connection,
    intent_id: pod0_domain::EffectIntentId,
) -> Result<AgentTurnId, StorageError> {
    let bytes: Option<Vec<u8>> = connection
            .query_row(
                "SELECT subject_id FROM pod0_effect_intents WHERE intent_id=?1 \
                 AND effect_kind_code=8 AND subject_code=4",
                [intent_id.into_bytes().as_slice()],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| StorageError::sqlite("read agent model effect subject", error))?;
    bytes
        .map(|value| {
            value
                .try_into()
                .map(AgentTurnId::from_bytes)
                .map_err(|_| StorageError::InvalidActivity)
        })
        .transpose()?
        .ok_or(StorageError::AgentTurnNotFound)
}

fn observation_fingerprint(input: &AgentModelObservationCommitInput) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/agent/model-effect-observation/v1");
    hash.update(input.lease.intent_id.into_bytes());
    hash.update(input.lease.attempt_id.into_bytes());
    hash.update(input.lease.lease_id.into_bytes());
    hash.update(input.lease.fence.to_be_bytes());
    hash.update(serde_json::to_vec(&input.observation).expect("typed durable observation"));
    ContentDigest::from_bytes(hash.finalize().into())
}

fn effect_error(error: EffectOutboxError) -> StorageError {
    match error {
        EffectOutboxError::StaleLease => StorageError::AgentTurnConflict,
        EffectOutboxError::InvalidRecord | EffectOutboxError::InvalidLeaseDuration => {
            StorageError::InvalidActivity
        }
        EffectOutboxError::Storage => StorageError::Sqlite {
            operation: "commit agent model effect observation",
        },
    }
}
