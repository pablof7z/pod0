use crate::{
    HostObservation, HostRequest, MAX_SCHEDULED_AGENT_CONTEXT_MESSAGE_BYTES,
    MAX_SCHEDULED_AGENT_CONTEXT_MESSAGES, MAX_SCHEDULED_AGENT_MODEL_BYTES,
    MAX_SCHEDULED_AGENT_OUTPUT_EXCERPT_BYTES, MAX_SCHEDULED_AGENT_PROMPT_BYTES,
    MAX_SCHEDULED_AGENT_PROVIDER_OPERATION_BYTES, MAX_SCHEDULED_AGENT_SAFE_DETAIL_BYTES,
    ScheduledAgentContextRole, ScheduledAgentExecutionObservation, scheduled_prompt_revision,
};

pub(super) fn scheduled_agent_observation_matches(
    request: &HostRequest,
    observation: &HostObservation,
) -> Option<bool> {
    let HostRequest::ExecuteScheduledAgentTurn { execution } = request else {
        return None;
    };
    let HostObservation::ScheduledAgentExecutionObserved { observation } = observation else {
        return Some(false);
    };
    let identity_matches = match observation {
        ScheduledAgentExecutionObservation::Accepted {
            occurrence_id,
            attempt_id,
            provider_operation_id,
        } => {
            provider_operation_id.as_ref().is_none_or(|value| {
                !value.is_empty() && value.len() <= MAX_SCHEDULED_AGENT_PROVIDER_OPERATION_BYTES
            }) && *occurrence_id == execution.occurrence_id
                && *attempt_id == execution.attempt_id
        }
        ScheduledAgentExecutionObservation::Completed {
            occurrence_id,
            attempt_id,
            output_excerpt,
            ..
        } => {
            *occurrence_id == execution.occurrence_id
                && *attempt_id == execution.attempt_id
                && !output_excerpt.trim().is_empty()
                && output_excerpt.len() <= MAX_SCHEDULED_AGENT_OUTPUT_EXCERPT_BYTES
                && u64::try_from(output_excerpt.len())
                    .is_ok_and(|size| size <= execution.maximum_output_bytes)
        }
        ScheduledAgentExecutionObservation::Failed {
            occurrence_id,
            attempt_id,
            safe_detail,
            retry_after_milliseconds,
            ..
        } => {
            *occurrence_id == execution.occurrence_id
                && *attempt_id == execution.attempt_id
                && safe_detail
                    .as_ref()
                    .is_none_or(|value| value.len() <= MAX_SCHEDULED_AGENT_SAFE_DETAIL_BYTES)
                && retry_after_milliseconds.is_none_or(|value| value <= 86_400_000)
        }
        ScheduledAgentExecutionObservation::Cancelled {
            occurrence_id,
            attempt_id,
        } => *occurrence_id == execution.occurrence_id && *attempt_id == execution.attempt_id,
        ScheduledAgentExecutionObservation::Unsupported { .. } => false,
    };
    Some(identity_matches && request_is_bounded(execution))
}

fn request_is_bounded(execution: &crate::ScheduledAgentExecutionRequest) -> bool {
    !execution.prompt.trim().is_empty()
        && execution.prompt.len() <= MAX_SCHEDULED_AGENT_PROMPT_BYTES
        && scheduled_prompt_revision(&execution.prompt) == Some(execution.prompt_revision)
        && !execution.model_reference.trim().is_empty()
        && execution.model_reference.len() <= MAX_SCHEDULED_AGENT_MODEL_BYTES
        && execution.context.len() <= MAX_SCHEDULED_AGENT_CONTEXT_MESSAGES
        && execution.context.iter().all(|message| {
            !matches!(message.role, ScheduledAgentContextRole::Unsupported { .. })
                && message.content.len() <= MAX_SCHEDULED_AGENT_CONTEXT_MESSAGE_BYTES
        })
        && execution.maximum_output_bytes > 0
        && execution.maximum_output_bytes <= MAX_SCHEDULED_AGENT_OUTPUT_EXCERPT_BYTES as u64
}
