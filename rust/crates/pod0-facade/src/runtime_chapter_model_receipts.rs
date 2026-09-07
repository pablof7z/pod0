use pod0_application::{
    ActivityFailureCode, ChapterModelFailureClassification, ChapterModelHostFailureCode,
    ChapterModelRetryDisposition, EffectOutcome, HostFailureCode, HostObservationReceipt,
    HostObservationRejection, MODEL_CHAPTER_REQUEST_DEADLINE_MILLISECONDS,
    ModelChapterWorkflowFailureCode, model_chapter_retry_delay_milliseconds,
};
use pod0_domain::{HostRequestId, StateRevision};
use pod0_storage::{ModelChapterFailureDisposition, ModelChapterWorkflowRecord, StorageError};

pub(super) fn failure_disposition(
    record: &ModelChapterWorkflowRecord,
    classification: ChapterModelFailureClassification,
    issued_revision: StateRevision,
    now_ms: i64,
    provider_retry_after_milliseconds: Option<i64>,
) -> ModelChapterFailureDisposition {
    use ModelChapterWorkflowFailureCode as C;
    if record.attempt >= record.max_attempts {
        return if record.may_have_submitted || classification.may_have_submitted {
            ModelChapterFailureDisposition::Ambiguous
        } else {
            ModelChapterFailureDisposition::Fail
        };
    }
    if classification.retry == ChapterModelRetryDisposition::AutomaticRequest
        && classification.resubmission_is_safe
    {
        let delay = model_chapter_retry_delay_milliseconds(
            record.attempt,
            provider_retry_after_milliseconds,
        );
        let not_before = now_ms.saturating_add(delay);
        return ModelChapterFailureDisposition::Retry {
            not_before_ms: not_before,
            deadline_at_ms: not_before.saturating_add(MODEL_CHAPTER_REQUEST_DEADLINE_MILLISECONDS),
            issued_revision,
            evidence_permits_resubmission: true,
        };
    }
    match classification.code {
        C::MissingCredential
        | C::ResponseTooLarge
        | C::InvalidResponse
        | C::QualificationRejected
        | C::ProviderRecoveryUnavailable => ModelChapterFailureDisposition::Block,
        C::InvalidRequest | C::ProviderRejected | C::Cancelled => {
            if classification.may_have_submitted {
                ModelChapterFailureDisposition::Ambiguous
            } else {
                ModelChapterFailureDisposition::Fail
            }
        }
        C::StaleTranscript | C::StalePublisherBase | C::SelectionChanged => {
            ModelChapterFailureDisposition::Replan
        }
        _ if classification.may_have_submitted => ModelChapterFailureDisposition::Ambiguous,
        _ => ModelChapterFailureDisposition::Fail,
    }
}

pub(super) fn generic_host_failure(code: HostFailureCode) -> ChapterModelHostFailureCode {
    match code {
        HostFailureCode::Offline => ChapterModelHostFailureCode::Offline,
        HostFailureCode::TimedOut => ChapterModelHostFailureCode::TimedOut,
        HostFailureCode::PermissionDenied => ChapterModelHostFailureCode::MissingCredential,
        HostFailureCode::InvalidResponse => ChapterModelHostFailureCode::InvalidResponse,
        HostFailureCode::ResponseTooLarge => ChapterModelHostFailureCode::ResponseTooLarge,
        _ => ChapterModelHostFailureCode::Transport,
    }
}

pub(super) fn model_failure_effect_outcome(
    classification: ChapterModelFailureClassification,
    disposition: &ModelChapterFailureDisposition,
    cancelled: bool,
) -> EffectOutcome {
    if matches!(disposition, ModelChapterFailureDisposition::Ambiguous) {
        EffectOutcome::OutcomeUnknown
    } else if cancelled {
        EffectOutcome::Cancelled
    } else {
        EffectOutcome::Failed {
            code: activity_failure(classification.code),
        }
    }
}

fn activity_failure(code: ModelChapterWorkflowFailureCode) -> ActivityFailureCode {
    use ModelChapterWorkflowFailureCode as Code;
    match code {
        Code::Offline => ActivityFailureCode::Offline,
        Code::TimedOut => ActivityFailureCode::TimedOut,
        Code::MissingCredential => ActivityFailureCode::PermissionDenied,
        Code::ResponseTooLarge => ActivityFailureCode::ResponseTooLarge,
        Code::ProviderUnavailable | Code::Transport => ActivityFailureCode::ProviderUnavailable,
        _ => ActivityFailureCode::InvalidResponse,
    }
}

pub(super) fn persisted(request_id: HostRequestId, terminal: bool) -> HostObservationReceipt {
    HostObservationReceipt::Persisted {
        request_id,
        terminal,
    }
}

pub(super) fn retain(request_id: HostRequestId) -> HostObservationReceipt {
    HostObservationReceipt::RetainAndRetry { request_id }
}

pub(super) fn rejected(
    request_id: HostRequestId,
    reason: HostObservationRejection,
) -> HostObservationReceipt {
    HostObservationReceipt::Rejected { request_id, reason }
}

pub(super) fn storage_receipt(
    request_id: HostRequestId,
    error: StorageError,
) -> HostObservationReceipt {
    match error {
        StorageError::ChapterWorkflowConflict | StorageError::ChapterWorkflowNotFound => {
            rejected(request_id, HostObservationRejection::StaleWorkflow)
        }
        _ => retain(request_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classification(code: ModelChapterWorkflowFailureCode) -> ChapterModelFailureClassification {
        ChapterModelFailureClassification {
            code,
            retry: ChapterModelRetryDisposition::ExplicitOnly,
            may_have_submitted: true,
            resubmission_is_safe: false,
        }
    }

    #[test]
    fn semantic_effect_outcomes_keep_failure_cancellation_and_ambiguity_distinct() {
        assert_eq!(
            model_failure_effect_outcome(
                classification(ModelChapterWorkflowFailureCode::AmbiguousSubmission),
                &ModelChapterFailureDisposition::Ambiguous,
                false,
            ),
            EffectOutcome::OutcomeUnknown
        );
        assert_eq!(
            model_failure_effect_outcome(
                classification(ModelChapterWorkflowFailureCode::Cancelled),
                &ModelChapterFailureDisposition::Fail,
                true,
            ),
            EffectOutcome::Cancelled
        );
        assert_eq!(
            model_failure_effect_outcome(
                classification(ModelChapterWorkflowFailureCode::Offline),
                &ModelChapterFailureDisposition::Block,
                false,
            ),
            EffectOutcome::Failed {
                code: ActivityFailureCode::Offline,
            }
        );
    }
}
