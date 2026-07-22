use std::collections::{BTreeMap, BTreeSet};

use pod0_application::{
    MAX_SCHEDULED_AGENT_CONTEXT_MESSAGES, MAX_SCHEDULED_AGENT_MODEL_BYTES,
    MAX_SCHEDULED_AGENT_OUTPUT_EXCERPT_BYTES, MAX_SCHEDULED_AGENT_SAFE_DETAIL_BYTES,
    MAX_SCHEDULED_AGENT_TASKS, ScheduledAgentExecutionObservation, ScheduledAgentExecutionRequest,
    ScheduledAgentFailureCode, ScheduledAgentStage, scheduled_attempt_id,
    scheduled_host_request_id, scheduled_occurrence_id, scheduled_prompt_revision,
    validate_scheduled_task_definition,
};
use rusqlite::Connection;

use crate::{
    LegacyScheduledAgentCutoverInput, LegacyScheduledAgentCutoverReport,
    MAX_LEGACY_SCHEDULED_AGENT_OCCURRENCES, StorageError,
};

pub(super) fn validate_input(input: &LegacyScheduledAgentCutoverInput) -> Result<(), StorageError> {
    if input.backup_byte_count == 0
        || input.observed_at.value() < 0
        || input.tasks.len() > usize::from(MAX_SCHEDULED_AGENT_TASKS)
        || input.occurrences.len() > MAX_LEGACY_SCHEDULED_AGENT_OCCURRENCES
    {
        return invalid("scheduled-agent cutover bounds are invalid");
    }
    let mut tasks = BTreeMap::new();
    for task in &input.tasks {
        let definition = &task.definition;
        if validate_scheduled_task_definition(definition).is_err()
            || definition.revision.value == 0
            || definition.created_at.value() < 0
            || definition.next_run_at.value() < 0
            || definition
                .last_run_at
                .is_some_and(|value| value.value() < 0)
            || tasks
                .insert(definition.task_id.into_bytes(), definition)
                .is_some()
        {
            return invalid("legacy scheduled task is invalid");
        }
    }
    let mut occurrences = BTreeSet::new();
    for occurrence in &input.occurrences {
        validate_occurrence(occurrence, &tasks)?;
        if !occurrences.insert(occurrence.state.occurrence_id.into_bytes()) {
            return invalid("legacy scheduled occurrence is duplicated");
        }
    }
    Ok(())
}

fn validate_occurrence(
    occurrence: &crate::LegacyScheduledAgentOccurrence,
    tasks: &BTreeMap<[u8; 16], &pod0_application::ScheduledTaskDefinition>,
) -> Result<(), StorageError> {
    let state = &occurrence.state;
    if !tasks.contains_key(&state.task_id.into_bytes())
        || state.occurrence_id != scheduled_occurrence_id(state.task_id, occurrence.scheduled_for)
        || occurrence.scheduled_for.value() < 0
        || occurrence.created_at.value() < 0
        || state.updated_at.value() < occurrence.created_at.value()
        || state.revision.value == 0
        || scheduled_prompt_revision(&state.prompt) != Some(state.prompt_revision)
        || state.model_reference.trim().is_empty()
        || state.model_reference.len() > MAX_SCHEDULED_AGENT_MODEL_BYTES
    {
        return invalid("legacy scheduled occurrence identity is invalid");
    }
    validate_attempt_identity(state)?;
    validate_stage(state, occurrence.output_excerpt.as_deref())
}

fn validate_attempt_identity(
    state: &pod0_application::ScheduledAgentOccurrenceState,
) -> Result<(), StorageError> {
    if state.attempt == 0 {
        if state.attempt_id.is_some()
            || state.request_id.is_some()
            || state.provider_operation_id.is_some()
        {
            return invalid("legacy scheduled attempt zero has provider identity");
        }
        return Ok(());
    }
    let expected_attempt = scheduled_attempt_id(state.occurrence_id, state.attempt)
        .ok_or(StorageError::ScheduledAgentWorkflowConflict)?;
    if state.attempt_id != Some(expected_attempt)
        || state.request_id != Some(scheduled_host_request_id(expected_attempt))
    {
        return invalid("legacy scheduled attempt identity is invalid");
    }
    Ok(())
}

fn validate_stage(
    state: &pod0_application::ScheduledAgentOccurrenceState,
    output_excerpt: Option<&str>,
) -> Result<(), StorageError> {
    match state.stage {
        ScheduledAgentStage::Pending => {
            if state.attempt != 0 || state.failure.is_some() || output_excerpt.is_some() {
                return invalid("legacy pending scheduled run is invalid");
            }
        }
        ScheduledAgentStage::RetryScheduled => {
            if state.attempt == 0
                || state.not_before.is_none()
                || !state
                    .failure
                    .as_ref()
                    .is_some_and(|failure| failure.retryable)
                || output_excerpt.is_some()
            {
                return invalid("legacy retry scheduled run is invalid");
            }
        }
        ScheduledAgentStage::Blocked => {
            if state.attempt == 0 || state.failure.is_none() || output_excerpt.is_some() {
                return invalid("legacy blocked scheduled run is invalid");
            }
        }
        ScheduledAgentStage::Cancelled => {
            if state.attempt == 0
                || !state.failure.as_ref().is_some_and(|failure| {
                    failure.code == ScheduledAgentFailureCode::Cancelled && !failure.retryable
                })
                || output_excerpt.is_some()
            {
                return invalid("legacy cancelled scheduled run is invalid");
            }
        }
        ScheduledAgentStage::Obsolete => {
            if state.attempt == 0 || output_excerpt.is_some() {
                return invalid("legacy obsolete scheduled run is invalid");
            }
        }
        ScheduledAgentStage::FailedPermanent | ScheduledAgentStage::Ambiguous => {
            if state.attempt == 0
                || state
                    .failure
                    .as_ref()
                    .is_none_or(|failure| failure.retryable)
                || output_excerpt.is_some()
            {
                return invalid("legacy terminal scheduled run is invalid");
            }
        }
        ScheduledAgentStage::Succeeded => validate_completion(state, output_excerpt)?,
        ScheduledAgentStage::Requested
        | ScheduledAgentStage::HostAccepted
        | ScheduledAgentStage::Unsupported { .. } => {
            return invalid("unsafe legacy scheduled stage cannot be imported");
        }
    }
    if state.failure.as_ref().is_some_and(|failure| {
        failure
            .safe_detail
            .as_ref()
            .is_some_and(|detail| detail.len() > MAX_SCHEDULED_AGENT_SAFE_DETAIL_BYTES)
    }) {
        return invalid("legacy scheduled failure detail is too large");
    }
    Ok(())
}

fn validate_completion(
    state: &pod0_application::ScheduledAgentOccurrenceState,
    output_excerpt: Option<&str>,
) -> Result<(), StorageError> {
    let output = output_excerpt.ok_or(StorageError::ScheduledAgentWorkflowConflict)?;
    if state.attempt == 0
        || state.failure.is_some()
        || output.is_empty()
        || output.len() > MAX_SCHEDULED_AGENT_OUTPUT_EXCERPT_BYTES
    {
        return invalid("legacy scheduled completion is invalid");
    }
    let request = ScheduledAgentExecutionRequest {
        occurrence_id: state.occurrence_id,
        attempt_id: state
            .attempt_id
            .ok_or(StorageError::ScheduledAgentWorkflowConflict)?,
        prompt_revision: state.prompt_revision,
        prompt: state.prompt.clone(),
        model_reference: state.model_reference.clone(),
        context: Vec::with_capacity(MAX_SCHEDULED_AGENT_CONTEXT_MESSAGES),
        maximum_output_bytes: MAX_SCHEDULED_AGENT_OUTPUT_EXCERPT_BYTES as u64,
    };
    let qualified = pod0_application::qualify_scheduled_agent_completion(&request, output)
        .ok_or(StorageError::ScheduledAgentWorkflowConflict)?;
    let ScheduledAgentExecutionObservation::Completed {
        artifact_id,
        output_digest,
        ..
    } = qualified
    else {
        return invalid("legacy scheduled completion qualification failed");
    };
    if state.artifact_id != Some(artifact_id) || state.output_digest != Some(output_digest) {
        return invalid("legacy scheduled completion evidence does not match output");
    }
    Ok(())
}

pub(super) fn verify_staged_rows(
    connection: &Connection,
    report: &LegacyScheduledAgentCutoverReport,
) -> Result<(), StorageError> {
    let expected_tasks = i64::from(report.task_count);
    let expected_occurrences = i64::from(report.occurrence_count);
    let task_count = count(connection, "pod0_scheduled_tasks")?;
    let occurrence_count = count(connection, "pod0_scheduled_occurrences")?;
    let attempt_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pod0_scheduled_occurrences WHERE attempt>0",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("verify staged scheduled attempts", error))?;
    let stored_attempt_count = count(connection, "pod0_scheduled_attempts")?;
    let succeeded_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pod0_scheduled_occurrences WHERE stage='succeeded'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("verify staged scheduled completions", error))?;
    let unsafe_count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM pod0_scheduled_occurrences WHERE stage IN('requested','host_accepted')",
        [],
        |row| row.get(0),
    ).map_err(|error| StorageError::sqlite("verify staged scheduled safety", error))?;
    let orphan_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pod0_scheduled_occurrences o LEFT JOIN pod0_scheduled_tasks t \
         ON t.task_id=o.task_id WHERE t.task_id IS NULL",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("verify staged scheduled ownership", error))?;
    if task_count != expected_tasks
        || occurrence_count != expected_occurrences
        || stored_attempt_count != attempt_count
        || count(connection, "pod0_scheduled_completion_evidence")? != succeeded_count
        || count(connection, "pod0_generated_artifacts")? != succeeded_count
        || unsafe_count != 0
        || orphan_count != 0
    {
        return Err(StorageError::ScheduledAgentWorkflowConflict);
    }
    Ok(())
}

fn count(connection: &Connection, table: &str) -> Result<i64, StorageError> {
    connection
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .map_err(|error| StorageError::sqlite("verify staged scheduled rows", error))
}

fn invalid<T>(detail: &'static str) -> Result<T, StorageError> {
    Err(StorageError::InvalidLegacyRecord {
        entity: "scheduled_agent",
        index: 0,
        detail,
    })
}
