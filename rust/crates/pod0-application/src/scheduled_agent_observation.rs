use crate::{
    MAX_SCHEDULED_AGENT_ATTEMPTS, MAX_SCHEDULED_AGENT_OUTPUT_EXCERPT_BYTES,
    MAX_SCHEDULED_AGENT_PROVIDER_OPERATION_BYTES, MAX_SCHEDULED_AGENT_SAFE_DETAIL_BYTES,
    SCHEDULED_AGENT_RETRY_DELAY_MILLISECONDS, ScheduledAgentExecutionObservation,
    ScheduledAgentFailure, ScheduledAgentFailureCode, ScheduledAgentOccurrenceState,
    ScheduledAgentStage, ScheduledAgentTransition, add_milliseconds, is_terminal, next_revision,
    scheduled_generated_artifact_id,
};
use pod0_domain::{ScheduledAttemptId, ScheduledOccurrenceId, UnixTimestampMilliseconds};
pub fn apply_scheduled_agent_observation(
    state: &mut ScheduledAgentOccurrenceState,
    observation: &ScheduledAgentExecutionObservation,
    observed_at: UnixTimestampMilliseconds,
) -> ScheduledAgentTransition {
    let Some((occurrence_id, attempt_id)) = observation_identity(observation) else {
        return ScheduledAgentTransition::RejectedInvalid;
    };
    if occurrence_id != state.occurrence_id || Some(attempt_id) != state.attempt_id {
        return ScheduledAgentTransition::IgnoredStale;
    }
    if is_terminal(state.stage) {
        return terminal_transition(state, observation);
    }
    if !observation_is_bounded(observation) {
        return ScheduledAgentTransition::RejectedInvalid;
    }
    match observation {
        ScheduledAgentExecutionObservation::Accepted {
            provider_operation_id,
            ..
        } => {
            if state.stage == ScheduledAgentStage::HostAccepted {
                return if state.provider_operation_id == *provider_operation_id {
                    ScheduledAgentTransition::IgnoredDuplicate
                } else {
                    ScheduledAgentTransition::RejectedInvalid
                };
            }
            if state.stage != ScheduledAgentStage::Requested {
                return ScheduledAgentTransition::IgnoredStale;
            }
            state.stage = ScheduledAgentStage::HostAccepted;
            state
                .provider_operation_id
                .clone_from(provider_operation_id);
        }
        ScheduledAgentExecutionObservation::Completed {
            artifact_id,
            output_digest,
            output_excerpt,
            ..
        } => {
            if *artifact_id != scheduled_generated_artifact_id(attempt_id)
                || !matches!(
                    state.stage,
                    ScheduledAgentStage::Requested | ScheduledAgentStage::HostAccepted
                )
                || output_excerpt.trim().is_empty()
            {
                return ScheduledAgentTransition::RejectedInvalid;
            }
            state.stage = ScheduledAgentStage::Succeeded;
            state.artifact_id = Some(*artifact_id);
            state.output_digest = Some(*output_digest);
            state.failure = None;
        }
        ScheduledAgentExecutionObservation::Failed {
            code,
            safe_detail,
            retry_after_milliseconds,
            ..
        } => {
            if matches!(
                state.stage,
                ScheduledAgentStage::RetryScheduled | ScheduledAgentStage::Blocked
            ) {
                return ScheduledAgentTransition::IgnoredDuplicate;
            }
            if !matches!(
                state.stage,
                ScheduledAgentStage::Requested | ScheduledAgentStage::HostAccepted
            ) {
                return ScheduledAgentTransition::IgnoredStale;
            }
            apply_failure(
                state,
                *code,
                safe_detail.clone(),
                *retry_after_milliseconds,
                observed_at,
            );
        }
        ScheduledAgentExecutionObservation::Cancelled { .. } => {
            if !matches!(
                state.stage,
                ScheduledAgentStage::Requested | ScheduledAgentStage::HostAccepted
            ) {
                return ScheduledAgentTransition::IgnoredStale;
            }
            state.stage = ScheduledAgentStage::Cancelled;
            state.failure = Some(failure(ScheduledAgentFailureCode::Cancelled, None, false));
        }
        ScheduledAgentExecutionObservation::Unsupported { .. } => {
            return ScheduledAgentTransition::RejectedInvalid;
        }
    }
    state.revision = next_revision(state.revision);
    state.updated_at = observed_at;
    ScheduledAgentTransition::Applied
}

fn terminal_transition(
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
    } else {
        ScheduledAgentTransition::RejectedInvalid
    }
}

pub fn cancel_scheduled_agent(
    state: &mut ScheduledAgentOccurrenceState,
    observed_at: UnixTimestampMilliseconds,
) -> ScheduledAgentTransition {
    if is_terminal(state.stage) {
        return ScheduledAgentTransition::IgnoredDuplicate;
    }
    state.stage = ScheduledAgentStage::Cancelled;
    state.failure = Some(failure(ScheduledAgentFailureCode::Cancelled, None, false));
    state.revision = next_revision(state.revision);
    state.updated_at = observed_at;
    ScheduledAgentTransition::Applied
}

pub fn mark_scheduled_agent_ambiguous_after_restart(
    state: &mut ScheduledAgentOccurrenceState,
    observed_at: UnixTimestampMilliseconds,
) -> ScheduledAgentTransition {
    if state.stage == ScheduledAgentStage::Ambiguous {
        return ScheduledAgentTransition::IgnoredDuplicate;
    }
    if state.stage != ScheduledAgentStage::HostAccepted {
        return ScheduledAgentTransition::IgnoredStale;
    }
    state.stage = ScheduledAgentStage::Ambiguous;
    state.failure = Some(failure(
        ScheduledAgentFailureCode::UnsafeToRetry,
        Some("Provider acceptance was persisted before process termination.".to_owned()),
        false,
    ));
    state.revision = next_revision(state.revision);
    state.updated_at = observed_at;
    ScheduledAgentTransition::Applied
}

fn observation_identity(
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

fn observation_is_bounded(observation: &ScheduledAgentExecutionObservation) -> bool {
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

fn apply_failure(
    state: &mut ScheduledAgentOccurrenceState,
    code: ScheduledAgentFailureCode,
    safe_detail: Option<String>,
    retry_after_milliseconds: Option<u64>,
    observed_at: UnixTimestampMilliseconds,
) {
    match code {
        ScheduledAgentFailureCode::MissingCredential => {
            state.stage = ScheduledAgentStage::Blocked;
            state.failure = Some(failure(code, safe_detail, true));
        }
        ScheduledAgentFailureCode::Offline
        | ScheduledAgentFailureCode::Network
        | ScheduledAgentFailureCode::RateLimited
        | ScheduledAgentFailureCode::ProviderUnavailable
        | ScheduledAgentFailureCode::Unexpected
            if state.attempt < MAX_SCHEDULED_AGENT_ATTEMPTS =>
        {
            let delay = retry_after_milliseconds
                .and_then(|value| i64::try_from(value).ok())
                .unwrap_or(SCHEDULED_AGENT_RETRY_DELAY_MILLISECONDS);
            state.stage = ScheduledAgentStage::RetryScheduled;
            state.not_before = Some(add_milliseconds(observed_at, delay));
            state.failure = Some(failure(code, safe_detail, true));
        }
        ScheduledAgentFailureCode::Cancelled => {
            state.stage = ScheduledAgentStage::Cancelled;
            state.failure = Some(failure(code, safe_detail, false));
        }
        ScheduledAgentFailureCode::Unsupported { .. } => {
            state.stage = ScheduledAgentStage::FailedPermanent;
            state.failure = Some(failure(code, safe_detail, false));
        }
        _ => {
            state.stage = ScheduledAgentStage::FailedPermanent;
            let terminal_code = if state.attempt >= MAX_SCHEDULED_AGENT_ATTEMPTS
                && matches!(
                    code,
                    ScheduledAgentFailureCode::Offline
                        | ScheduledAgentFailureCode::Network
                        | ScheduledAgentFailureCode::RateLimited
                        | ScheduledAgentFailureCode::ProviderUnavailable
                        | ScheduledAgentFailureCode::Unexpected
                ) {
                ScheduledAgentFailureCode::RetryExhausted
            } else {
                code
            };
            state.failure = Some(failure(terminal_code, safe_detail, false));
        }
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
