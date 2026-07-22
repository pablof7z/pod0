use crate::{
    MAX_SCHEDULED_AGENT_OUTPUT_EXCERPT_BYTES, MAX_SCHEDULED_AGENT_PROVIDER_OPERATION_BYTES,
    MAX_SCHEDULED_AGENT_SAFE_DETAIL_BYTES, ScheduledAgentExecutionObservation,
    ScheduledAgentOccurrenceState, ScheduledAgentStage, ScheduledAgentTransition,
};
use pod0_domain::{ScheduledAttemptId, ScheduledOccurrenceId};

pub(super) fn terminal_transition(
    state: &ScheduledAgentOccurrenceState,
    observation: &ScheduledAgentExecutionObservation,
) -> ScheduledAgentTransition {
    let exact_replay = match observation {
        ScheduledAgentExecutionObservation::Completed {
            artifact_id,
            output_digest,
            ..
        } => {
            state.stage == ScheduledAgentStage::Succeeded
                && state.artifact_id == Some(*artifact_id)
                && state.output_digest == Some(*output_digest)
        }
        ScheduledAgentExecutionObservation::Cancelled { .. } => {
            state.stage == ScheduledAgentStage::Cancelled
        }
        ScheduledAgentExecutionObservation::Failed {
            code, safe_detail, ..
        } => state
            .failure
            .as_ref()
            .is_some_and(|failure| failure.code == *code && failure.safe_detail == *safe_detail),
        ScheduledAgentExecutionObservation::Accepted { .. }
        | ScheduledAgentExecutionObservation::Unsupported { .. } => false,
    };
    if exact_replay {
        ScheduledAgentTransition::IgnoredDuplicate
    } else if matches!(
        (state.stage, observation),
        (
            ScheduledAgentStage::Succeeded,
            ScheduledAgentExecutionObservation::Completed { .. }
        ) | (
            ScheduledAgentStage::Cancelled,
            ScheduledAgentExecutionObservation::Cancelled { .. }
        ) | (
            ScheduledAgentStage::FailedPermanent,
            ScheduledAgentExecutionObservation::Failed { .. }
        )
    ) {
        ScheduledAgentTransition::RejectedInvalid
    } else {
        ScheduledAgentTransition::IgnoredStale
    }
}

pub(super) fn observation_identity(
    observation: &ScheduledAgentExecutionObservation,
) -> Option<(ScheduledOccurrenceId, ScheduledAttemptId)> {
    match observation {
        ScheduledAgentExecutionObservation::Accepted {
            occurrence_id,
            attempt_id,
            ..
        }
        | ScheduledAgentExecutionObservation::Completed {
            occurrence_id,
            attempt_id,
            ..
        }
        | ScheduledAgentExecutionObservation::Failed {
            occurrence_id,
            attempt_id,
            ..
        }
        | ScheduledAgentExecutionObservation::Cancelled {
            occurrence_id,
            attempt_id,
        } => Some((*occurrence_id, *attempt_id)),
        ScheduledAgentExecutionObservation::Unsupported { .. } => None,
    }
}

pub(super) fn observation_is_bounded(observation: &ScheduledAgentExecutionObservation) -> bool {
    match observation {
        ScheduledAgentExecutionObservation::Accepted {
            provider_operation_id,
            ..
        } => provider_operation_id.as_ref().is_none_or(|value| {
            !value.is_empty() && value.len() <= MAX_SCHEDULED_AGENT_PROVIDER_OPERATION_BYTES
        }),
        ScheduledAgentExecutionObservation::Completed { output_excerpt, .. } => {
            !output_excerpt.is_empty()
                && output_excerpt.len() <= MAX_SCHEDULED_AGENT_OUTPUT_EXCERPT_BYTES
        }
        ScheduledAgentExecutionObservation::Failed {
            safe_detail,
            retry_after_milliseconds,
            ..
        } => {
            safe_detail
                .as_ref()
                .is_none_or(|value| value.len() <= MAX_SCHEDULED_AGENT_SAFE_DETAIL_BYTES)
                && retry_after_milliseconds.is_none_or(|value| value <= 86_400_000)
        }
        ScheduledAgentExecutionObservation::Cancelled { .. } => true,
        ScheduledAgentExecutionObservation::Unsupported { .. } => false,
    }
}
