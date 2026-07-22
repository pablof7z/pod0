use pod0_application::{
    ChapterModelFailureClassification, ChapterModelHostFailureCode, ChapterModelRetryDisposition,
    HostFailureCode, HostObservation, HostObservationEnvelope, HostObservationReceipt,
    HostObservationRejection, MODEL_CHAPTER_REQUEST_DEADLINE_MILLISECONDS,
    ModelChapterWorkflowFailureCode, model_chapter_retry_delay_milliseconds,
};
use pod0_domain::{ContentDigest, HostRequestId, StateRevision};
use pod0_storage::{
    ModelChapterCompletionRecord, ModelChapterFailureDisposition, ModelChapterWorkflowRecord,
    StorageError,
};
use sha2::{Digest as _, Sha256};

use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn late_ambiguous_model_record(
        &self,
        observation: &HostObservationEnvelope,
    ) -> Option<ModelChapterWorkflowRecord> {
        let episode_id = match observation.observation {
            HostObservation::ChapterModelProviderAccepted { episode_id, .. }
            | HostObservation::ChapterModelCompleted { episode_id, .. }
            | HostObservation::ChapterModelFailed { episode_id, .. } => episode_id,
            _ => return None,
        };
        let record = self
            .store
            .as_ref()?
            .model_chapter_workflow(episode_id)
            .ok()??;
        (record.state == pod0_storage::ModelChapterWorkflowState::Ambiguous
            && record.request_id == Some(observation.request_id)
            && record.cancellation_id == observation.cancellation_id
            && record.issued_revision == observation.observed_request_revision)
            .then_some(record)
    }
}

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

pub(super) fn completion_observation_replays(
    existing: &ModelChapterCompletionRecord,
    envelope: &HostObservationEnvelope,
    episode_id: pod0_domain::EpisodeId,
    generation: u64,
    fence: pod0_domain::ChapterModelSubmissionFenceId,
    value: &pod0_application::ChapterModelCompletionObservation,
) -> bool {
    existing.request_id == envelope.request_id
        && existing.episode_id == episode_id
        && existing.generation == generation
        && existing.submission_fence_id == fence
        && existing.completion == value.completion
        && existing.completion_digest
            == ContentDigest::from_bytes(Sha256::digest(value.completion.as_bytes()).into())
        && existing.provider == value.provider
        && existing.model == value.model
        && existing.prompt_tokens == value.prompt_tokens
        && existing.completion_tokens == value.completion_tokens
        && existing.cached_tokens == value.cached_tokens
        && existing.reasoning_tokens == value.reasoning_tokens
        && existing.cost_microusd == value.cost_microusd
        && value
            .provider_operation_id
            .as_ref()
            .is_none_or(|value| existing.provider_operation_id.as_ref() == Some(value))
        && value
            .provider_status
            .as_ref()
            .is_none_or(|value| existing.provider_status.as_ref() == Some(value))
        && value
            .provider_generated_at
            .is_none_or(|generated| existing.generated_at_ms == generated.value)
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

pub(super) fn core_failure(
    code: ModelChapterWorkflowFailureCode,
) -> pod0_application::CoreFailureCode {
    match code {
        ModelChapterWorkflowFailureCode::StaleTranscript
        | ModelChapterWorkflowFailureCode::StalePublisherBase
        | ModelChapterWorkflowFailureCode::SelectionChanged => {
            pod0_application::CoreFailureCode::RevisionConflict
        }
        ModelChapterWorkflowFailureCode::Offline
        | ModelChapterWorkflowFailureCode::TimedOut
        | ModelChapterWorkflowFailureCode::Transport
        | ModelChapterWorkflowFailureCode::ProviderUnavailable => {
            pod0_application::CoreFailureCode::HostUnavailable
        }
        _ => pod0_application::CoreFailureCode::HostRejected,
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
