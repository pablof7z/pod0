use pod0_application::{
    ScheduledAgentAttemptPlan, ScheduledAgentOccurrenceState, begin_scheduled_agent_attempt,
    reconcile_scheduled_occurrence,
};
use pod0_domain::ScheduledOccurrenceId;
use rusqlite::{Connection, Transaction, params};

use crate::scheduled_agent_store_read::{active_tasks, pending_requests, read_occurrence};
use crate::scheduled_agent_store_tasks::{
    command_receipt, finish_command, to_i64, validate_context,
};
use crate::{
    ScheduledAgentCommandContext, ScheduledAgentReconcileOutcome, ScheduledAgentStore, StorageError,
};

impl ScheduledAgentStore {
    pub fn reconcile_due_runs(
        &self,
        context: ScheduledAgentCommandContext,
    ) -> Result<ScheduledAgentReconcileOutcome, StorageError> {
        validate_context(&context)?;
        self.write(|transaction| {
            if command_receipt(transaction, &context)?.is_some() {
                return Ok(ScheduledAgentReconcileOutcome {
                    created_occurrences: Vec::new(),
                    requests: pending_requests(transaction, Some(context.command_id), u16::MAX)?,
                });
            }
            let mut created_occurrences = Vec::new();
            for definition in active_tasks(transaction)? {
                let Some(occurrence) =
                    reconcile_scheduled_occurrence(&definition, context.observed_at)
                        .map_err(|_| StorageError::ScheduledAgentWorkflowConflict)?
                else {
                    continue;
                };
                if read_occurrence(transaction, occurrence.occurrence_id)?.is_none() {
                    insert_occurrence(transaction, &definition, &occurrence)?;
                    created_occurrences.push(occurrence.occurrence_id);
                }
            }
            let candidates = retry_candidates(transaction, context.observed_at.value())?;
            for occurrence_id in candidates {
                let occurrence = read_occurrence(transaction, occurrence_id)?
                    .ok_or(StorageError::ScheduledAgentWorkflowNotFound)?;
                let plan = begin_scheduled_agent_attempt(&occurrence, context.observed_at)
                    .map_err(|_| StorageError::ScheduledAgentWorkflowConflict)?;
                persist_attempt(transaction, &context, &occurrence, &plan)?;
            }
            finish_command(transaction, &context, None, None)?;
            Ok(ScheduledAgentReconcileOutcome {
                created_occurrences,
                requests: pending_requests(transaction, Some(context.command_id), u16::MAX)?,
            })
        })
    }
}

pub(crate) fn persist_occurrence_state(
    transaction: &Transaction<'_>,
    previous: &ScheduledAgentOccurrenceState,
    next: &ScheduledAgentOccurrenceState,
) -> Result<(), StorageError> {
    let stage = crate::scheduled_agent_store_codec::stage_wire(next.stage)
        .ok_or(StorageError::ScheduledAgentWorkflowConflict)?;
    let (failure_code, failure_wire, failure_detail, failure_retryable) = next
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
        .unwrap_or((None, None, None, false));
    transaction
        .execute(
            "UPDATE pod0_scheduled_occurrences SET stage=?1,workflow_revision=?2,attempt=?3,\
         attempt_id=?4,request_id=?5,provider_operation_id=?6,not_before_ms=?7,artifact_id=?8,\
         output_digest=?9,failure_code=?10,failure_wire_code=?11,failure_detail=?12,\
         failure_retryable=?13,updated_at_ms=?14 WHERE occurrence_id=?15 AND workflow_revision=?16",
            params![
                stage,
                to_i64(next.revision.value)?,
                i64::from(next.attempt),
                next.attempt_id.map(|value| value.into_bytes().to_vec()),
                next.request_id.map(|value| value.into_bytes().to_vec()),
                next.provider_operation_id,
                next.not_before.map(|value| value.value()),
                next.artifact_id.map(|value| value.into_bytes().to_vec()),
                next.output_digest.map(|value| value.into_bytes().to_vec()),
                failure_code,
                failure_wire,
                failure_detail,
                failure_retryable,
                next.updated_at.value(),
                next.occurrence_id.into_bytes().as_slice(),
                to_i64(previous.revision.value)?,
            ],
        )
        .map_err(|error| StorageError::sqlite("persist scheduled occurrence", error))?;
    if transaction.changes() != 1 {
        return Err(StorageError::ScheduledAgentWorkflowConflict);
    }
    Ok(())
}

fn insert_occurrence(
    transaction: &Transaction<'_>,
    definition: &pod0_application::ScheduledTaskDefinition,
    occurrence: &ScheduledAgentOccurrenceState,
) -> Result<(), StorageError> {
    transaction.execute(
        "INSERT INTO pod0_scheduled_occurrences(occurrence_id,task_id,scheduled_for_ms,prompt,\
         prompt_revision,model_reference,stage,workflow_revision,attempt,attempt_id,request_id,\
         provider_operation_id,not_before_ms,artifact_id,output_digest,failure_code,\
         failure_wire_code,failure_detail,failure_retryable,created_at_ms,updated_at_ms) \
         VALUES(?1,?2,?3,?4,?5,?6,'pending',?7,0,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,0,?8,?8)",
        params![
            occurrence.occurrence_id.into_bytes().as_slice(),
            occurrence.task_id.into_bytes().as_slice(), definition.next_run_at.value(),
            occurrence.prompt, occurrence.prompt_revision.into_bytes().as_slice(),
            occurrence.model_reference, to_i64(occurrence.revision.value)?,
            occurrence.updated_at.value(),
        ],
    ).map_err(|error| StorageError::sqlite("insert scheduled occurrence", error))?;
    Ok(())
}

fn persist_attempt(
    transaction: &Transaction<'_>,
    context: &ScheduledAgentCommandContext,
    previous: &ScheduledAgentOccurrenceState,
    plan: &ScheduledAgentAttemptPlan,
) -> Result<(), StorageError> {
    persist_occurrence_state(transaction, previous, &plan.state)?;
    let attempt_id = plan
        .state
        .attempt_id
        .ok_or(StorageError::ScheduledAgentWorkflowConflict)?;
    transaction.execute(
        "INSERT INTO pod0_scheduled_attempts(attempt_id,occurrence_id,attempt,request_id,command_id,\
         cancellation_id,issued_revision,deadline_at_ms,state,provider_operation_id,\
         last_sequence_number,last_observation_fingerprint,failure_code,failure_wire_code,\
         failure_detail,failure_retryable,created_at_ms,updated_at_ms) \
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'requested',NULL,NULL,NULL,NULL,NULL,NULL,0,?9,?9)",
        params![
            attempt_id.into_bytes().as_slice(), plan.state.occurrence_id.into_bytes().as_slice(),
            i64::from(plan.state.attempt), plan.request_id.into_bytes().as_slice(),
            context.command_id.into_bytes().as_slice(),
            context.cancellation_id.into_bytes().as_slice(),
            to_i64(context.issued_revision.value)?, plan.deadline_at.value(),
            context.observed_at.value(),
        ],
    ).map_err(|error| StorageError::sqlite("insert scheduled attempt", error))?;
    Ok(())
}

fn retry_candidates(
    connection: &Connection,
    now_ms: i64,
) -> Result<Vec<ScheduledOccurrenceId>, StorageError> {
    let mut statement = connection
        .prepare(
            "SELECT occurrence_id FROM pod0_scheduled_occurrences WHERE stage='pending' \
         OR (stage='retry_scheduled' AND not_before_ms<=?1) ORDER BY updated_at_ms,occurrence_id",
        )
        .map_err(|error| StorageError::sqlite("prepare due scheduled runs", error))?;
    let rows = statement
        .query_map([now_ms], |row| row.get::<_, Vec<u8>>(0))
        .map_err(|error| StorageError::sqlite("query due scheduled runs", error))?;
    rows.map(|row| {
        let bytes = row.map_err(|error| StorageError::sqlite("read due scheduled run", error))?;
        crate::scheduled_agent_store_codec::occurrence_id(&bytes)
    })
    .collect()
}
