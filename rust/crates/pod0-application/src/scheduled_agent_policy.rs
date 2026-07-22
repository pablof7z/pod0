use pod0_domain::{
    GeneratedArtifactId, HostRequestId, ScheduledAttemptId, ScheduledOccurrenceId, StateRevision,
    UnixTimestampMilliseconds,
};

use crate::{
    MAX_SCHEDULED_AGENT_ATTEMPTS, MAX_SCHEDULED_AGENT_LABEL_BYTES, MAX_SCHEDULED_AGENT_MODEL_BYTES,
    MAX_SCHEDULED_AGENT_OUTPUT_EXCERPT_BYTES, MAX_SCHEDULED_AGENT_PROMPT_BYTES,
    SCHEDULED_AGENT_HOST_DEADLINE_MILLISECONDS, ScheduledAgentAllowedActions,
    ScheduledAgentExecutionRequest, ScheduledAgentFailure, ScheduledAgentStage,
    ScheduledAgentWorkflowProjection, ScheduledTaskDefinition, scheduled_attempt_id,
    scheduled_host_request_id, scheduled_occurrence_id, scheduled_prompt_revision,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduledAgentOccurrenceState {
    pub task_id: pod0_domain::ScheduledTaskId,
    pub occurrence_id: ScheduledOccurrenceId,
    pub prompt: String,
    pub prompt_revision: pod0_domain::ContentDigest,
    pub model_reference: String,
    pub stage: ScheduledAgentStage,
    pub revision: StateRevision,
    pub attempt: u16,
    pub attempt_id: Option<ScheduledAttemptId>,
    pub request_id: Option<HostRequestId>,
    pub provider_operation_id: Option<String>,
    pub not_before: Option<UnixTimestampMilliseconds>,
    pub artifact_id: Option<GeneratedArtifactId>,
    pub output_digest: Option<pod0_domain::ContentDigest>,
    pub failure: Option<ScheduledAgentFailure>,
    pub updated_at: UnixTimestampMilliseconds,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduledAgentAttemptPlan {
    pub state: ScheduledAgentOccurrenceState,
    pub request_id: HostRequestId,
    pub deadline_at: UnixTimestampMilliseconds,
    pub request: ScheduledAgentExecutionRequest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScheduledAgentTransition {
    Applied,
    IgnoredDuplicate,
    IgnoredStale,
    RejectedInvalid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScheduledAgentPolicyError {
    InvalidDefinition,
    NotReady,
    Terminal,
    RetryExhausted,
}

pub fn reconcile_scheduled_occurrence(
    definition: &ScheduledTaskDefinition,
    observed_at: UnixTimestampMilliseconds,
) -> Result<Option<ScheduledAgentOccurrenceState>, ScheduledAgentPolicyError> {
    validate_scheduled_task_definition(definition)?;
    if definition.next_run_at > observed_at {
        return Ok(None);
    }
    Ok(Some(ScheduledAgentOccurrenceState {
        task_id: definition.task_id,
        occurrence_id: scheduled_occurrence_id(definition.task_id, definition.next_run_at),
        prompt: definition.prompt.clone(),
        prompt_revision: definition.prompt_revision,
        model_reference: definition.model_reference.clone(),
        stage: ScheduledAgentStage::Pending,
        revision: StateRevision::new(1),
        attempt: 0,
        attempt_id: None,
        request_id: None,
        provider_operation_id: None,
        not_before: None,
        artifact_id: None,
        output_digest: None,
        failure: None,
        updated_at: observed_at,
    }))
}

pub fn begin_scheduled_agent_attempt(
    state: &ScheduledAgentOccurrenceState,
    observed_at: UnixTimestampMilliseconds,
) -> Result<ScheduledAgentAttemptPlan, ScheduledAgentPolicyError> {
    if is_terminal(state.stage) {
        return Err(ScheduledAgentPolicyError::Terminal);
    }
    if !matches!(
        state.stage,
        ScheduledAgentStage::Pending | ScheduledAgentStage::RetryScheduled
    ) || state.not_before.is_some_and(|value| value > observed_at)
    {
        return Err(ScheduledAgentPolicyError::NotReady);
    }
    let attempt = state
        .attempt
        .checked_add(1)
        .filter(|value| *value <= MAX_SCHEDULED_AGENT_ATTEMPTS)
        .ok_or(ScheduledAgentPolicyError::RetryExhausted)?;
    let attempt_id = scheduled_attempt_id(state.occurrence_id, attempt)
        .ok_or(ScheduledAgentPolicyError::RetryExhausted)?;
    let request_id = scheduled_host_request_id(attempt_id);
    let mut next = state.clone();
    next.stage = ScheduledAgentStage::Requested;
    next.revision = next_revision(state.revision);
    next.attempt = attempt;
    next.attempt_id = Some(attempt_id);
    next.request_id = Some(request_id);
    next.provider_operation_id = None;
    next.not_before = None;
    next.failure = None;
    next.updated_at = observed_at;
    Ok(ScheduledAgentAttemptPlan {
        state: next,
        request_id,
        deadline_at: add_milliseconds(observed_at, SCHEDULED_AGENT_HOST_DEADLINE_MILLISECONDS),
        request: ScheduledAgentExecutionRequest {
            occurrence_id: state.occurrence_id,
            attempt_id,
            prompt_revision: state.prompt_revision,
            prompt: state.prompt.clone(),
            model_reference: state.model_reference.clone(),
            context: Vec::new(),
            maximum_output_bytes: MAX_SCHEDULED_AGENT_OUTPUT_EXCERPT_BYTES as u64,
        },
    })
}

pub fn advance_scheduled_task_after_completion(
    definition: &ScheduledTaskDefinition,
    occurrence: &ScheduledAgentOccurrenceState,
    completed_at: UnixTimestampMilliseconds,
) -> Result<ScheduledTaskDefinition, ScheduledAgentPolicyError> {
    validate_scheduled_task_definition(definition)?;
    if occurrence.stage != ScheduledAgentStage::Succeeded
        || occurrence.occurrence_id
            != scheduled_occurrence_id(definition.task_id, definition.next_run_at)
    {
        return Err(ScheduledAgentPolicyError::NotReady);
    }
    let interval = i64::try_from(definition.interval_milliseconds).unwrap_or(i64::MAX);
    let mut next = definition.clone();
    next.last_run_at = Some(completed_at);
    next.next_run_at = add_milliseconds(completed_at, interval);
    next.revision = next_revision(definition.revision);
    Ok(next)
}

impl ScheduledAgentOccurrenceState {
    #[must_use]
    pub fn projection(&self) -> ScheduledAgentWorkflowProjection {
        ScheduledAgentWorkflowProjection {
            task_id: self.task_id,
            occurrence_id: self.occurrence_id,
            prompt_revision: self.prompt_revision,
            stage: self.stage,
            workflow_revision: self.revision,
            attempt: self.attempt,
            attempt_id: self.attempt_id,
            request_id: self.request_id,
            not_before: self.not_before,
            artifact_id: self.artifact_id,
            output_digest: self.output_digest,
            failure: self.failure.clone(),
            updated_at: self.updated_at,
            allowed_actions: ScheduledAgentAllowedActions {
                can_retry: self.failure.as_ref().is_some_and(|value| value.retryable)
                    && matches!(
                        self.stage,
                        ScheduledAgentStage::RetryScheduled | ScheduledAgentStage::Blocked
                    ),
                can_cancel: !is_terminal(self.stage),
            },
        }
    }
}

pub fn validate_scheduled_task_definition(
    definition: &ScheduledTaskDefinition,
) -> Result<(), ScheduledAgentPolicyError> {
    let valid = !definition.label.trim().is_empty()
        && definition.label.len() <= MAX_SCHEDULED_AGENT_LABEL_BYTES
        && !definition.prompt.trim().is_empty()
        && definition.prompt.len() <= MAX_SCHEDULED_AGENT_PROMPT_BYTES
        && scheduled_prompt_revision(&definition.prompt) == Some(definition.prompt_revision)
        && !definition.model_reference.trim().is_empty()
        && definition.model_reference.len() <= MAX_SCHEDULED_AGENT_MODEL_BYTES
        && definition.interval_milliseconds > 0;
    if valid {
        Ok(())
    } else {
        Err(ScheduledAgentPolicyError::InvalidDefinition)
    }
}

pub(crate) fn is_terminal(stage: ScheduledAgentStage) -> bool {
    matches!(
        stage,
        ScheduledAgentStage::Cancelled
            | ScheduledAgentStage::Obsolete
            | ScheduledAgentStage::FailedPermanent
            | ScheduledAgentStage::Succeeded
    )
}

pub(crate) fn next_revision(revision: StateRevision) -> StateRevision {
    StateRevision::new(revision.value.saturating_add(1))
}

pub(crate) fn add_milliseconds(
    observed_at: UnixTimestampMilliseconds,
    milliseconds: i64,
) -> UnixTimestampMilliseconds {
    UnixTimestampMilliseconds::new(observed_at.value().saturating_add(milliseconds))
}
