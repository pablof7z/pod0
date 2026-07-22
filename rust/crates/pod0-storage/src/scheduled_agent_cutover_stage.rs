use pod0_application::{
    SCHEDULED_AGENT_HOST_DEADLINE_MILLISECONDS, ScheduledAgentExecutionObservation,
    ScheduledAgentOccurrenceState, ScheduledAgentStage,
};
use pod0_domain::{CancellationId, CommandId};
use rusqlite::{Transaction, params};

use crate::{LegacyScheduledAgentCutoverInput, StorageError};

pub(super) fn stage_rows(
    transaction: &Transaction<'_>,
    input: &LegacyScheduledAgentCutoverInput,
) -> Result<(), StorageError> {
    let mut tasks: Vec<_> = input.tasks.iter().collect();
    tasks.sort_by_key(|task| task.definition.task_id.into_bytes());
    for task in tasks {
        insert_task(transaction, &task.definition)?;
    }
    let mut occurrences: Vec<_> = input.occurrences.iter().collect();
    occurrences.sort_by_key(|occurrence| occurrence.state.occurrence_id.into_bytes());
    for occurrence in occurrences {
        insert_occurrence(transaction, occurrence)?;
    }
    Ok(())
}

pub(super) fn clear_staged_rows(transaction: &Transaction<'_>) -> Result<(), StorageError> {
    for table in [
        "pod0_generated_artifacts",
        "pod0_scheduled_completion_evidence",
        "pod0_scheduled_attempts",
        "pod0_scheduled_occurrences",
        "pod0_scheduled_command_receipts",
        "pod0_scheduled_tasks",
    ] {
        transaction
            .execute(&format!("DELETE FROM {table}"), [])
            .map_err(|error| StorageError::sqlite("clear staged scheduled-agent rows", error))?;
    }
    Ok(())
}

fn insert_task(
    transaction: &Transaction<'_>,
    definition: &pod0_application::ScheduledTaskDefinition,
) -> Result<(), StorageError> {
    transaction.execute(
        "INSERT INTO pod0_scheduled_tasks(task_id,label,prompt,prompt_revision,model_reference,\
         interval_ms,task_revision,last_run_at_ms,next_run_at_ms,active,created_at_ms,updated_at_ms,\
         removed_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,1,?10,?11,NULL)",
        params![
            definition.task_id.into_bytes().as_slice(),
            definition.label,
            definition.prompt,
            definition.prompt_revision.into_bytes().as_slice(),
            definition.model_reference,
            to_i64(definition.interval_milliseconds)?,
            to_i64(definition.revision.value)?,
            definition.last_run_at.map(|value| value.value()),
            definition.next_run_at.value(),
            definition.created_at.value(),
            definition.created_at.value().max(
                definition.last_run_at.map_or(definition.created_at.value(), |value| value.value()),
            ),
        ],
    ).map_err(|error| StorageError::sqlite("stage legacy scheduled task", error))?;
    Ok(())
}

fn insert_occurrence(
    transaction: &Transaction<'_>,
    occurrence: &crate::LegacyScheduledAgentOccurrence,
) -> Result<(), StorageError> {
    let state = &occurrence.state;
    let stage = crate::scheduled_agent_store_codec::stage_wire(state.stage)
        .ok_or(StorageError::ScheduledAgentWorkflowConflict)?;
    let failure = failure_parts(state);
    transaction
        .execute(
            "INSERT INTO pod0_scheduled_occurrences(occurrence_id,task_id,scheduled_for_ms,prompt,\
         prompt_revision,model_reference,stage,workflow_revision,attempt,attempt_id,request_id,\
         provider_operation_id,not_before_ms,artifact_id,output_digest,failure_code,\
         failure_wire_code,failure_detail,failure_retryable,created_at_ms,updated_at_ms) \
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21)",
            params![
                state.occurrence_id.into_bytes().as_slice(),
                state.task_id.into_bytes().as_slice(),
                occurrence.scheduled_for.value(),
                state.prompt,
                state.prompt_revision.into_bytes().as_slice(),
                state.model_reference,
                stage,
                to_i64(state.revision.value)?,
                i64::from(state.attempt),
                state.attempt_id.map(|value| value.into_bytes().to_vec()),
                state.request_id.map(|value| value.into_bytes().to_vec()),
                state.provider_operation_id,
                state.not_before.map(|value| value.value()),
                state.artifact_id.map(|value| value.into_bytes().to_vec()),
                state.output_digest.map(|value| value.into_bytes().to_vec()),
                failure.0,
                failure.1,
                failure.2,
                failure.3,
                occurrence.created_at.value(),
                state.updated_at.value(),
            ],
        )
        .map_err(|error| StorageError::sqlite("stage legacy scheduled occurrence", error))?;
    if state.attempt > 0 {
        insert_attempt(transaction, state, occurrence.output_excerpt.as_deref())?;
    }
    Ok(())
}

fn insert_attempt(
    transaction: &Transaction<'_>,
    state: &ScheduledAgentOccurrenceState,
    output_excerpt: Option<&str>,
) -> Result<(), StorageError> {
    let attempt_id = state
        .attempt_id
        .ok_or(StorageError::ScheduledAgentWorkflowConflict)?;
    let request_id = state
        .request_id
        .ok_or(StorageError::ScheduledAgentWorkflowConflict)?;
    let attempt_state = attempt_state(state.stage)?;
    let failure = failure_parts(state);
    let completion = completion_observation(state, output_excerpt)?;
    let fingerprint = completion
        .as_ref()
        .map(crate::scheduled_agent_store_observation_fingerprint::observation_fingerprint);
    let command_id = CommandId::from_bytes(attempt_id.into_bytes());
    let mut cancellation_bytes = attempt_id.into_bytes();
    cancellation_bytes[0] ^= 0xff;
    let cancellation_id = CancellationId::from_bytes(cancellation_bytes);
    transaction.execute(
        "INSERT INTO pod0_scheduled_attempts(attempt_id,occurrence_id,attempt,request_id,command_id,\
         cancellation_id,issued_revision,deadline_at_ms,state,provider_operation_id,\
         last_sequence_number,last_observation_fingerprint,failure_code,failure_wire_code,\
         failure_detail,failure_retryable,created_at_ms,updated_at_ms) \
         VALUES(?1,?2,?3,?4,?5,?6,0,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
        params![
            attempt_id.into_bytes().as_slice(),
            state.occurrence_id.into_bytes().as_slice(),
            i64::from(state.attempt),
            request_id.into_bytes().as_slice(),
            command_id.into_bytes().as_slice(),
            cancellation_id.into_bytes().as_slice(),
            state.updated_at.value().saturating_add(SCHEDULED_AGENT_HOST_DEADLINE_MILLISECONDS),
            attempt_state,
            state.provider_operation_id,
            completion.as_ref().map(|_| 0_i64),
            fingerprint.map(|value| value.to_vec()),
            failure.0,
            failure.1,
            failure.2,
            failure.3,
            state.updated_at.value(),
            state.updated_at.value(),
        ],
    ).map_err(|error| StorageError::sqlite("stage legacy scheduled attempt", error))?;
    if let Some(ScheduledAgentExecutionObservation::Completed {
        artifact_id,
        output_digest,
        output_excerpt,
        ..
    }) = completion
    {
        insert_completion(
            transaction,
            state,
            artifact_id,
            output_digest,
            &output_excerpt,
            fingerprint.ok_or(StorageError::ScheduledAgentWorkflowConflict)?,
        )?;
    }
    Ok(())
}

fn insert_completion(
    transaction: &Transaction<'_>,
    state: &ScheduledAgentOccurrenceState,
    artifact_id: pod0_domain::GeneratedArtifactId,
    output_digest: pod0_domain::ContentDigest,
    output_excerpt: &str,
    fingerprint: [u8; 32],
) -> Result<(), StorageError> {
    let attempt_id = state
        .attempt_id
        .ok_or(StorageError::ScheduledAgentWorkflowConflict)?;
    let request_id = state
        .request_id
        .ok_or(StorageError::ScheduledAgentWorkflowConflict)?;
    transaction
        .execute(
            "INSERT INTO pod0_scheduled_completion_evidence(attempt_id,occurrence_id,request_id,\
         artifact_id,output_digest,output_excerpt,sequence_number,observation_fingerprint,\
         observed_at_ms,state,committed_at_ms) VALUES(?1,?2,?3,?4,?5,?6,0,?7,?8,'committed',?8)",
            params![
                attempt_id.into_bytes().as_slice(),
                state.occurrence_id.into_bytes().as_slice(),
                request_id.into_bytes().as_slice(),
                artifact_id.into_bytes().as_slice(),
                output_digest.into_bytes().as_slice(),
                output_excerpt,
                fingerprint.as_slice(),
                state.updated_at.value()
            ],
        )
        .map_err(|error| StorageError::sqlite("stage legacy scheduled completion", error))?;
    transaction.execute(
        "INSERT INTO pod0_generated_artifacts(artifact_id,occurrence_id,attempt_id,kind,\
         content_digest,bounded_excerpt,selected_at_ms) VALUES(?1,?2,?3,'scheduled_agent_output',?4,?5,?6)",
        params![artifact_id.into_bytes().as_slice(), state.occurrence_id.into_bytes().as_slice(),
            attempt_id.into_bytes().as_slice(), output_digest.into_bytes().as_slice(),
            output_excerpt, state.updated_at.value()],
    ).map_err(|error| StorageError::sqlite("stage legacy generated artifact", error))?;
    Ok(())
}

fn completion_observation(
    state: &ScheduledAgentOccurrenceState,
    output_excerpt: Option<&str>,
) -> Result<Option<ScheduledAgentExecutionObservation>, StorageError> {
    if state.stage != ScheduledAgentStage::Succeeded {
        return Ok(None);
    }
    Ok(Some(ScheduledAgentExecutionObservation::Completed {
        occurrence_id: state.occurrence_id,
        attempt_id: state
            .attempt_id
            .ok_or(StorageError::ScheduledAgentWorkflowConflict)?,
        artifact_id: state
            .artifact_id
            .ok_or(StorageError::ScheduledAgentWorkflowConflict)?,
        output_digest: state
            .output_digest
            .ok_or(StorageError::ScheduledAgentWorkflowConflict)?,
        output_excerpt: output_excerpt
            .ok_or(StorageError::ScheduledAgentWorkflowConflict)?
            .to_owned(),
    }))
}

fn failure_parts(
    state: &ScheduledAgentOccurrenceState,
) -> (Option<&str>, Option<i64>, Option<&str>, bool) {
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
        ScheduledAgentStage::RetryScheduled => Ok("retry_scheduled"),
        ScheduledAgentStage::Blocked => Ok("blocked"),
        ScheduledAgentStage::Cancelled | ScheduledAgentStage::Obsolete => Ok("cancelled"),
        ScheduledAgentStage::FailedPermanent => Ok("failed"),
        ScheduledAgentStage::Succeeded => Ok("succeeded"),
        ScheduledAgentStage::Ambiguous => Ok("ambiguous"),
        _ => Err(StorageError::ScheduledAgentWorkflowConflict),
    }
}

fn to_i64(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::ScheduledAgentWorkflowConflict)
}
