use pod0_application::{
    MAX_SCHEDULED_AGENT_OUTPUT_EXCERPT_BYTES, ScheduledAgentExecutionObservation,
    ScheduledAgentExecutionRequest, ScheduledAgentFailure, ScheduledAgentFailureCode,
    ScheduledAgentOccurrenceState, ScheduledAgentStage, ScheduledTaskDefinition,
    scheduled_attempt_id, scheduled_host_request_id, scheduled_occurrence_id,
    scheduled_prompt_revision,
};
use pod0_domain::{ContentDigest, StateRevision, UnixTimestampMilliseconds};
use pod0_storage::{
    LegacyScheduledAgentCutoverInput, LegacyScheduledAgentOccurrence, LegacyScheduledAgentTask,
    StorageError,
};

use crate::{
    LegacyScheduledAgentOccurrenceDisposition, LegacyScheduledAgentOccurrenceInput,
    LegacyScheduledAgentTaskInput,
};

pub(super) fn cutover_input(
    backup_digest: ContentDigest,
    backup_byte_count: u64,
    tasks: Vec<LegacyScheduledAgentTaskInput>,
    occurrences: Vec<LegacyScheduledAgentOccurrenceInput>,
    observed_at: UnixTimestampMilliseconds,
) -> Result<LegacyScheduledAgentCutoverInput, StorageError> {
    let tasks = tasks
        .into_iter()
        .map(|input| {
            let prompt_revision = scheduled_prompt_revision(&input.prompt)
                .ok_or(StorageError::ScheduledAgentWorkflowConflict)?;
            Ok(LegacyScheduledAgentTask {
                definition: ScheduledTaskDefinition {
                    task_id: input.task_id,
                    label: input.label,
                    prompt: input.prompt,
                    prompt_revision,
                    model_reference: input.model_reference,
                    interval_milliseconds: input.interval_milliseconds,
                    created_at: input.created_at,
                    last_run_at: input.last_run_at,
                    next_run_at: input.next_run_at,
                    revision: StateRevision::new(1),
                },
            })
        })
        .collect::<Result<Vec<_>, StorageError>>()?;
    let occurrences = occurrences
        .into_iter()
        .map(map_occurrence)
        .collect::<Result<Vec<_>, StorageError>>()?;
    Ok(LegacyScheduledAgentCutoverInput {
        backup_digest,
        backup_byte_count,
        tasks,
        occurrences,
        observed_at,
    })
}

fn map_occurrence(
    input: LegacyScheduledAgentOccurrenceInput,
) -> Result<LegacyScheduledAgentOccurrence, StorageError> {
    let prompt_revision = scheduled_prompt_revision(&input.prompt)
        .ok_or(StorageError::ScheduledAgentWorkflowConflict)?;
    let occurrence_id = scheduled_occurrence_id(input.task_id, input.scheduled_for);
    let (stage, attempt, not_before, failure, output_excerpt) = disposition(input.disposition)?;
    let attempt_id = scheduled_attempt_id(occurrence_id, attempt);
    let mut state = ScheduledAgentOccurrenceState {
        task_id: input.task_id,
        occurrence_id,
        prompt: input.prompt,
        prompt_revision,
        model_reference: input.model_reference,
        stage,
        revision: StateRevision::new(u64::from(attempt).saturating_add(1)),
        attempt,
        attempt_id,
        request_id: attempt_id.map(scheduled_host_request_id),
        provider_operation_id: None,
        not_before,
        artifact_id: None,
        output_digest: None,
        failure,
        updated_at: input.updated_at,
    };
    if let Some(output) = output_excerpt.as_deref() {
        qualify_completion(&mut state, output)?;
    }
    Ok(LegacyScheduledAgentOccurrence {
        scheduled_for: input.scheduled_for,
        created_at: input.created_at,
        state,
        output_excerpt,
    })
}

type Disposition = (
    ScheduledAgentStage,
    u16,
    Option<UnixTimestampMilliseconds>,
    Option<ScheduledAgentFailure>,
    Option<String>,
);

fn disposition(
    value: LegacyScheduledAgentOccurrenceDisposition,
) -> Result<Disposition, StorageError> {
    let result = match value {
        LegacyScheduledAgentOccurrenceDisposition::Pending => {
            (ScheduledAgentStage::Pending, 0, None, None, None)
        }
        LegacyScheduledAgentOccurrenceDisposition::RetryScheduled {
            attempt,
            not_before,
            failure_code,
            safe_detail,
        } => (
            ScheduledAgentStage::RetryScheduled,
            attempt,
            Some(not_before),
            Some(failure(failure_code, safe_detail, true)),
            None,
        ),
        LegacyScheduledAgentOccurrenceDisposition::Blocked {
            attempt,
            failure_code,
            safe_detail,
            retryable,
        } => (
            ScheduledAgentStage::Blocked,
            attempt,
            None,
            Some(failure(failure_code, safe_detail, retryable)),
            None,
        ),
        LegacyScheduledAgentOccurrenceDisposition::Ambiguous {
            attempt,
            safe_detail,
        } => (
            ScheduledAgentStage::Ambiguous,
            attempt,
            None,
            Some(failure(
                ScheduledAgentFailureCode::UnsafeToRetry,
                safe_detail,
                false,
            )),
            None,
        ),
        LegacyScheduledAgentOccurrenceDisposition::FailedPermanent {
            attempt,
            failure_code,
            safe_detail,
        } => (
            ScheduledAgentStage::FailedPermanent,
            attempt,
            None,
            Some(failure(failure_code, safe_detail, false)),
            None,
        ),
        LegacyScheduledAgentOccurrenceDisposition::Cancelled { attempt } => (
            ScheduledAgentStage::Cancelled,
            attempt,
            None,
            Some(failure(ScheduledAgentFailureCode::Cancelled, None, false)),
            None,
        ),
        LegacyScheduledAgentOccurrenceDisposition::Obsolete { attempt } => {
            (ScheduledAgentStage::Obsolete, attempt, None, None, None)
        }
        LegacyScheduledAgentOccurrenceDisposition::Succeeded {
            attempt,
            output_excerpt,
        } => (
            ScheduledAgentStage::Succeeded,
            attempt,
            None,
            None,
            Some(output_excerpt),
        ),
    };
    if result.1 > pod0_application::MAX_SCHEDULED_AGENT_ATTEMPTS {
        Err(StorageError::ScheduledAgentWorkflowConflict)
    } else {
        Ok(result)
    }
}

fn failure(
    code: ScheduledAgentFailureCode,
    safe_detail: Option<String>,
    retryable: bool,
) -> ScheduledAgentFailure {
    ScheduledAgentFailure {
        code,
        safe_detail,
        retryable,
    }
}

fn qualify_completion(
    state: &mut ScheduledAgentOccurrenceState,
    output: &str,
) -> Result<(), StorageError> {
    let request = ScheduledAgentExecutionRequest {
        occurrence_id: state.occurrence_id,
        attempt_id: state
            .attempt_id
            .ok_or(StorageError::ScheduledAgentWorkflowConflict)?,
        prompt_revision: state.prompt_revision,
        prompt: state.prompt.clone(),
        model_reference: state.model_reference.clone(),
        context: Vec::new(),
        maximum_output_bytes: MAX_SCHEDULED_AGENT_OUTPUT_EXCERPT_BYTES as u64,
    };
    let observation = pod0_application::qualify_scheduled_agent_completion(&request, output)
        .ok_or(StorageError::ScheduledAgentWorkflowConflict)?;
    let ScheduledAgentExecutionObservation::Completed {
        artifact_id,
        output_digest,
        ..
    } = observation
    else {
        return Err(StorageError::ScheduledAgentWorkflowConflict);
    };
    state.artifact_id = Some(artifact_id);
    state.output_digest = Some(output_digest);
    Ok(())
}
