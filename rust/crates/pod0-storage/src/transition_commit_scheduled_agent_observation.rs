use pod0_application::{
    ActivityFailureCode, EffectOutcome, RequestDisposition, ScheduledAgentActivityTransition,
    ScheduledAgentExecutionObservation, ScheduledAgentTransition,
    ScheduledObservationActivityInput, apply_scheduled_agent_observation,
    plan_scheduled_observation,
};
use pod0_domain::{CommandId, ContentDigest, EffectAttemptId};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::scheduled_agent_store_observations::{apply_observation_in_transaction, read_attempt};
use crate::scheduled_agent_store_read::read_occurrence;
use crate::{
    EffectOutboxError, ScheduledAgentLeasedObservationInput, ScheduledAgentObservationOutcome,
    StorageError, TransitionIngress, TransitionIngressKind,
};

pub(crate) fn commit_scheduled_agent_observation(
    path: &std::path::Path,
    input: ScheduledAgentLeasedObservationInput,
) -> Result<ScheduledAgentObservationOutcome, StorageError> {
    let lease = input.lease;
    let observation = input.observation.clone();
    let applied = std::cell::Cell::new(false);
    let terminal =
        crate::effect_outbox::scheduled_observation_is_terminal(&observation.observation);
    let planned_outcome = std::cell::Cell::new(EffectOutcome::Progressed);
    let receipt = TransitionCommit::open(path)?.commit_planned_with_transaction_hooks(
        TransitionIngress {
            kind: TransitionIngressKind::HostObservation,
            id: observation_ingress_id(lease.attempt_id, observation.sequence_number),
            fingerprint: observation_fingerprint(&input),
        },
        input.committed_at,
        |transaction| {
            let occurrence_id = observation_occurrence(&observation.observation)?;
            crate::effect_outbox::validate_scheduled_agent_lease_in_transaction(
                transaction,
                lease,
                &observation,
                occurrence_id,
            )
            .map_err(effect_error)?;
            let attempt = read_attempt(transaction, observation.request_id)?
                .ok_or(StorageError::StaleScheduledAgentAttempt)?;
            if attempt.occurrence_id != occurrence_id
                || attempt.cancellation_id != observation.cancellation_id
                || attempt.issued_revision != observation.observed_request_revision
            {
                return Err(StorageError::StaleScheduledAgentAttempt);
            }
            let before = read_occurrence(transaction, attempt.occurrence_id)?
                .ok_or(StorageError::ScheduledAgentWorkflowNotFound)?;
            let mut after = before.clone();
            let transition = if attempt
                .last_sequence_number
                .is_some_and(|sequence| observation.sequence_number < sequence)
            {
                ScheduledAgentTransition::IgnoredStale
            } else {
                apply_scheduled_agent_observation(
                    &mut after,
                    &observation.observation,
                    observation.observed_at,
                )
            };
            let disposition = match transition {
                ScheduledAgentTransition::Applied => RequestDisposition::Accepted,
                ScheduledAgentTransition::IgnoredDuplicate => RequestDisposition::Duplicate,
                ScheduledAgentTransition::IgnoredStale => RequestDisposition::Stale,
                ScheduledAgentTransition::RejectedInvalid => {
                    return Err(StorageError::ScheduledAgentWorkflowConflict);
                }
            };
            applied.set(disposition == RequestDisposition::Accepted);
            let outcome = effect_outcome(&observation.observation);
            planned_outcome.set(outcome);
            plan_scheduled_observation(ScheduledObservationActivityInput {
                command_id: CommandId::from_bytes(observation.request_id.into_bytes()),
                request_id: observation.request_id,
                occurrence_id: attempt.occurrence_id,
                current_revision: before.revision,
                committed_revision: if applied.get() {
                    after.revision
                } else {
                    before.revision
                },
                intent_id: lease.intent_id,
                observation_activity_id: observation_activity_id(
                    lease.attempt_id,
                    observation.sequence_number,
                ),
                attempt_id: lease.attempt_id,
                authorizing_activity_id: lease.authorizing_activity_id,
                correlation_id: lease.correlation_id,
                outcome,
                transition: observation_transition(&observation.observation),
                disposition,
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction| {
            if !applied.get() {
                return Ok(());
            }
            crate::effect_outbox::stage_scheduled_agent_observation_in_transaction(
                transaction,
                lease,
                &observation,
                observation_occurrence(&observation.observation)?,
                planned_outcome.get(),
                terminal,
            )
            .map_err(effect_error)
        },
        |transaction, expected, _| {
            if !applied.get() {
                return Ok(expected);
            }
            match apply_observation_in_transaction(transaction, &observation)? {
                ScheduledAgentObservationOutcome::Updated(state) => Ok(state.revision),
                ScheduledAgentObservationOutcome::Duplicate(_)
                | ScheduledAgentObservationOutcome::Stale => {
                    Err(StorageError::ScheduledAgentWorkflowConflict)
                }
            }
        },
        |transaction| {
            if applied.get() && terminal {
                crate::effect_outbox::complete_host_observation_in_transaction(transaction, lease)
                    .map_err(effect_error)?;
            }
            Ok(())
        },
    )?;
    let occurrence_id = observation_occurrence(&input.observation.observation)?;
    let state = crate::ScheduledAgentStore::open_authoritative(path)?
        .occurrence(occurrence_id)?
        .ok_or(StorageError::ScheduledAgentWorkflowNotFound)?;
    if receipt.replayed {
        Ok(ScheduledAgentObservationOutcome::Duplicate(state))
    } else if applied.get() {
        Ok(ScheduledAgentObservationOutcome::Updated(state))
    } else {
        Ok(ScheduledAgentObservationOutcome::Stale)
    }
}

fn observation_occurrence(
    observation: &ScheduledAgentExecutionObservation,
) -> Result<pod0_domain::ScheduledOccurrenceId, StorageError> {
    match observation {
        ScheduledAgentExecutionObservation::Accepted { occurrence_id, .. }
        | ScheduledAgentExecutionObservation::Completed { occurrence_id, .. }
        | ScheduledAgentExecutionObservation::Failed { occurrence_id, .. }
        | ScheduledAgentExecutionObservation::Cancelled { occurrence_id, .. } => Ok(*occurrence_id),
        ScheduledAgentExecutionObservation::Unsupported { .. } => {
            Err(StorageError::ScheduledAgentWorkflowConflict)
        }
    }
}

fn observation_transition(
    observation: &ScheduledAgentExecutionObservation,
) -> ScheduledAgentActivityTransition {
    match observation {
        ScheduledAgentExecutionObservation::Completed { .. } => {
            ScheduledAgentActivityTransition::ArtifactAdopted
        }
        _ => ScheduledAgentActivityTransition::AttemptStateChanged,
    }
}

fn effect_outcome(observation: &ScheduledAgentExecutionObservation) -> EffectOutcome {
    use pod0_application::ScheduledAgentFailureCode as Failure;
    match observation {
        ScheduledAgentExecutionObservation::Accepted { .. } => EffectOutcome::Progressed,
        ScheduledAgentExecutionObservation::Completed { .. } => EffectOutcome::Succeeded,
        ScheduledAgentExecutionObservation::Cancelled { .. } => EffectOutcome::Cancelled,
        ScheduledAgentExecutionObservation::Unsupported { wire_code } => EffectOutcome::Failed {
            code: ActivityFailureCode::Unsupported {
                wire_code: *wire_code,
            },
        },
        ScheduledAgentExecutionObservation::Failed { code, .. } => EffectOutcome::Failed {
            code: match code {
                Failure::Offline => ActivityFailureCode::Offline,
                Failure::PermissionDenied => ActivityFailureCode::PermissionDenied,
                Failure::InvalidOutput => ActivityFailureCode::InvalidResponse,
                Failure::MissingCredential => ActivityFailureCode::Unauthorized,
                Failure::Network
                | Failure::RateLimited
                | Failure::ProviderUnavailable
                | Failure::UnsafeToRetry
                | Failure::Unexpected
                | Failure::RetryExhausted => ActivityFailureCode::ProviderUnavailable,
                Failure::StorageUnavailable => ActivityFailureCode::StorageUnavailable,
                Failure::Cancelled => return EffectOutcome::Cancelled,
                Failure::Unsupported { wire_code } => ActivityFailureCode::Unsupported {
                    wire_code: *wire_code,
                },
            },
        },
    }
}

fn observation_activity_id(attempt_id: EffectAttemptId, sequence: u64) -> EffectAttemptId {
    EffectAttemptId::from_bytes(observation_ingress_id(attempt_id, sequence))
}

fn observation_ingress_id(attempt_id: EffectAttemptId, sequence: u64) -> [u8; 16] {
    let mut hash = Sha256::new();
    hash.update(b"pod0/scheduled-agent/observation-ingress/v1");
    hash.update(attempt_id.into_bytes());
    hash.update(sequence.to_be_bytes());
    hash.finalize()[..16]
        .try_into()
        .expect("fixed digest prefix")
}

fn observation_fingerprint(input: &ScheduledAgentLeasedObservationInput) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/scheduled-agent/leased-observation/v1");
    hash.update(input.lease.intent_id.into_bytes());
    hash.update(input.lease.attempt_id.into_bytes());
    hash.update(input.observation.sequence_number.to_be_bytes());
    hash.update(
        crate::scheduled_agent_store_observation_fingerprint::observation_fingerprint(
            &input.observation.observation,
        ),
    );
    ContentDigest::from_bytes(hash.finalize().into())
}

fn effect_error(error: EffectOutboxError) -> StorageError {
    match error {
        EffectOutboxError::StaleLease => StorageError::StaleScheduledAgentAttempt,
        EffectOutboxError::InvalidRecord | EffectOutboxError::InvalidLeaseDuration => {
            StorageError::InvalidActivity
        }
        EffectOutboxError::Storage => StorageError::Sqlite {
            operation: "commit scheduled-agent effect observation",
        },
    }
}
