use pod0_application::{
    ChapterModelFailureClassification, ChapterModelHostFailureCode, ChapterModelRetryDisposition,
    HostFailureCode, HostObservationReceipt, HostObservationRejection,
    MODEL_CHAPTER_REQUEST_DEADLINE_MILLISECONDS, ModelChapterWorkflowFailureCode,
    model_chapter_retry_delay_milliseconds,
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
