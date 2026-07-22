use pod0_application::{
    ScheduledAgentExecutionObservation, ScheduledAgentOccurrenceState, ScheduledAgentStage,
    ScheduledAgentTransition, apply_scheduled_agent_observation, cancel_scheduled_agent,
};
use pod0_domain::{ScheduledOccurrenceId, StateRevision};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::scheduled_agent_store_completion::commit_completion;
use crate::scheduled_agent_store_observation_fingerprint::observation_fingerprint;
use crate::scheduled_agent_store_read::read_occurrence;
use crate::scheduled_agent_store_reconcile::persist_occurrence_state;
use crate::scheduled_agent_store_tasks::{
    command_receipt, finish_command, to_i64, validate_context,
};
use crate::{
    ScheduledAgentCommandContext, ScheduledAgentObservationInput, ScheduledAgentObservationOutcome,
    ScheduledAgentStore, StorageError,
};

type AttemptDatabaseRow = (
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    i64,
    Option<i64>,
    Option<Vec<u8>>,
);

impl ScheduledAgentStore {
    pub fn apply_observation(
        &self,
        input: ScheduledAgentObservationInput,
    ) -> Result<ScheduledAgentObservationOutcome, StorageError> {
        if input.observed_at.value() < 0 {
            return Err(StorageError::ScheduledAgentWorkflowConflict);
        }
        self.write(|transaction| apply_observation(transaction, &input))
    }

    pub fn cancel_occurrence(
        &self,
        context: ScheduledAgentCommandContext,
        occurrence_id: ScheduledOccurrenceId,
        expected_revision: StateRevision,
    ) -> Result<ScheduledAgentOccurrenceState, StorageError> {
        validate_context(&context)?;
        self.write(|transaction| {
            if let Some(receipt) = command_receipt(transaction, &context)? {
                let id = receipt.occurrence_id
                    .ok_or(StorageError::ScheduledAgentCommandConflict)?;
                return read_occurrence(transaction, id)?
                    .ok_or(StorageError::ScheduledAgentWorkflowNotFound);
            }
            let previous = read_occurrence(transaction, occurrence_id)?
                .ok_or(StorageError::ScheduledAgentWorkflowNotFound)?;
            if previous.revision != expected_revision {
                return Err(StorageError::ScheduledAgentWorkflowConflict);
            }
            let mut next = previous.clone();
            match cancel_scheduled_agent(&mut next, context.observed_at) {
                ScheduledAgentTransition::Applied => {}
                ScheduledAgentTransition::IgnoredDuplicate => return Ok(previous),
                _ => return Err(StorageError::ScheduledAgentWorkflowConflict),
            }
            persist_occurrence_state(transaction, &previous, &next)?;
            if let Some(attempt_id) = next.attempt_id {
                transaction.execute(
                    "UPDATE pod0_scheduled_attempts SET state='cancelled',failure_code='cancelled',\
                     failure_wire_code=NULL,failure_detail=NULL,failure_retryable=0,updated_at_ms=?1 \
                     WHERE attempt_id=?2 AND state NOT IN('succeeded','failed','cancelled','ambiguous')",
                    params![context.observed_at.value(), attempt_id.into_bytes().as_slice()],
                ).map_err(|error| StorageError::sqlite("cancel scheduled attempt", error))?;
            }
            finish_command(transaction, &context, Some(next.task_id), Some(occurrence_id))?;
            Ok(next)
        })
    }
}

fn apply_observation(
    transaction: &Transaction<'_>,
    input: &ScheduledAgentObservationInput,
) -> Result<ScheduledAgentObservationOutcome, StorageError> {
    let attempt = read_attempt(transaction, input.request_id)?
        .ok_or(StorageError::StaleScheduledAgentAttempt)?;
    if attempt.cancellation_id != input.cancellation_id
        || attempt.issued_revision != input.observed_request_revision
    {
        return Ok(ScheduledAgentObservationOutcome::Stale);
    }
    let fingerprint = observation_fingerprint(&input.observation);
    if let Some(sequence) = attempt.last_sequence_number {
        if input.sequence_number < sequence {
            return Ok(ScheduledAgentObservationOutcome::Stale);
        }
        if input.sequence_number == sequence {
            if attempt.last_fingerprint == Some(fingerprint) {
                let state = read_occurrence(transaction, attempt.occurrence_id)?
                    .ok_or(StorageError::ScheduledAgentWorkflowNotFound)?;
                return Ok(ScheduledAgentObservationOutcome::Duplicate(state));
            }
            return Err(StorageError::ScheduledAgentWorkflowConflict);
        }
    }
    let previous = read_occurrence(transaction, attempt.occurrence_id)?
        .ok_or(StorageError::ScheduledAgentWorkflowNotFound)?;
    let mut next = previous.clone();
    match apply_scheduled_agent_observation(&mut next, &input.observation, input.observed_at) {
        ScheduledAgentTransition::IgnoredDuplicate => {
            return Ok(ScheduledAgentObservationOutcome::Duplicate(previous));
        }
        ScheduledAgentTransition::IgnoredStale => {
            return Ok(ScheduledAgentObservationOutcome::Stale);
        }
        ScheduledAgentTransition::RejectedInvalid => {
            return Err(StorageError::ScheduledAgentWorkflowConflict);
        }
        ScheduledAgentTransition::Applied => {}
    }
    persist_occurrence_state(transaction, &previous, &next)?;
    if let ScheduledAgentExecutionObservation::Completed {
        artifact_id,
        output_digest,
        output_excerpt,
        ..
    } = &input.observation
    {
        commit_completion(
            transaction,
            &attempt,
            &previous,
            &next,
            *artifact_id,
            *output_digest,
            output_excerpt,
            input,
            fingerprint,
        )?;
    } else {
        update_attempt(transaction, &attempt, &next, input, fingerprint)?;
    }
    Ok(ScheduledAgentObservationOutcome::Updated(next))
}

#[derive(Clone)]
pub(crate) struct AttemptRow {
    pub(crate) occurrence_id: ScheduledOccurrenceId,
    pub(crate) attempt_id: pod0_domain::ScheduledAttemptId,
    pub(crate) request_id: pod0_domain::HostRequestId,
    pub(crate) cancellation_id: pod0_domain::CancellationId,
    pub(crate) issued_revision: StateRevision,
    pub(crate) last_sequence_number: Option<u64>,
    pub(crate) last_fingerprint: Option<[u8; 32]>,
}

fn read_attempt(
    transaction: &Transaction<'_>,
    request_id: pod0_domain::HostRequestId,
) -> Result<Option<AttemptRow>, StorageError> {
    let row: Option<AttemptDatabaseRow> = transaction
        .query_row(
            "SELECT occurrence_id,attempt_id,request_id,cancellation_id,issued_revision,\
             last_sequence_number,last_observation_fingerprint FROM pod0_scheduled_attempts \
             WHERE request_id=?1",
            [request_id.into_bytes().as_slice()],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read scheduled attempt", error))?;
    row.map(
        |(occurrence, attempt, request, cancellation, revision, sequence, fingerprint)| {
            Ok(AttemptRow {
                occurrence_id: crate::scheduled_agent_store_codec::occurrence_id(&occurrence)?,
                attempt_id: crate::scheduled_agent_store_codec::attempt_id(&attempt)?,
                request_id: crate::scheduled_agent_store_codec::request_id(&request)?,
                cancellation_id: crate::scheduled_agent_store_codec::cancellation_id(
                    &cancellation,
                )?,
                issued_revision: crate::scheduled_agent_store_codec::revision(revision)?,
                last_sequence_number: sequence
                    .map(|value| {
                        u64::try_from(value).map_err(|_| StorageError::CorruptSchema {
                            detail: "scheduled sequence is malformed",
                        })
                    })
                    .transpose()?,
                last_fingerprint: fingerprint
                    .map(|value| {
                        value.try_into().map_err(|_| StorageError::CorruptSchema {
                            detail: "scheduled observation fingerprint is malformed",
                        })
                    })
                    .transpose()?,
            })
        },
    )
    .transpose()
}

fn update_attempt(
    transaction: &Transaction<'_>,
    attempt: &AttemptRow,
    next: &ScheduledAgentOccurrenceState,
    input: &ScheduledAgentObservationInput,
    fingerprint: [u8; 32],
) -> Result<(), StorageError> {
    let (failure_code, failure_wire, failure_detail, retryable) = failure_columns(next);
    transaction
        .execute(
            "UPDATE pod0_scheduled_attempts SET state=?1,provider_operation_id=?2,\
         last_sequence_number=?3,last_observation_fingerprint=?4,failure_code=?5,\
         failure_wire_code=?6,failure_detail=?7,failure_retryable=?8,updated_at_ms=?9 \
         WHERE attempt_id=?10",
            params![
                attempt_state(next.stage)?,
                next.provider_operation_id,
                to_i64(input.sequence_number)?,
                fingerprint.as_slice(),
                failure_code,
                failure_wire,
                failure_detail,
                retryable,
                input.observed_at.value(),
                attempt.attempt_id.into_bytes().as_slice(),
            ],
        )
        .map_err(|error| StorageError::sqlite("update scheduled attempt", error))?;
    Ok(())
}

fn failure_columns(
    state: &ScheduledAgentOccurrenceState,
) -> (Option<&'static str>, Option<i64>, Option<&str>, bool) {
    state
        .failure
        .as_ref()
        .map(|failure| {
            let (code, wire) = crate::scheduled_agent_store_codec::failure_wire(failure.code);
            (
                Some(code),
                wire,
                failure.safe_detail.as_deref(),
                failure.retryable,
            )
        })
        .unwrap_or((None, None, None, false))
}

fn attempt_state(stage: ScheduledAgentStage) -> Result<&'static str, StorageError> {
    match stage {
        ScheduledAgentStage::Requested => Ok("requested"),
        ScheduledAgentStage::HostAccepted => Ok("host_accepted"),
        ScheduledAgentStage::RetryScheduled => Ok("retry_scheduled"),
        ScheduledAgentStage::Blocked => Ok("blocked"),
        ScheduledAgentStage::Cancelled | ScheduledAgentStage::Obsolete => Ok("cancelled"),
        ScheduledAgentStage::FailedPermanent => Ok("failed"),
        ScheduledAgentStage::Succeeded => Ok("succeeded"),
        ScheduledAgentStage::Ambiguous => Ok("ambiguous"),
        _ => Err(StorageError::ScheduledAgentWorkflowConflict),
    }
}
