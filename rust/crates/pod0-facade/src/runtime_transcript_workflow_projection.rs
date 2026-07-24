use pod0_application::{
    CoreFailureCode, TranscriptProvider, TranscriptWorkflowFailure, TranscriptWorkflowFailureCode,
    TranscriptWorkflowOrigin, TranscriptWorkflowProjection, TranscriptWorkflowStage,
    TranscriptWorkflowsProjection, transcript_allowed_actions,
};
use pod0_domain::{EpisodeId, UnixTimestampMilliseconds};
use pod0_storage::{StoredTranscriptWorkflowStage, TranscriptWorkflowRecord};

use crate::runtime_state::{FacadeState, failure};

impl FacadeState {
    pub(super) fn transcript_workflows_projection(
        &self,
        episode_id: Option<EpisodeId>,
        offset: usize,
        maximum_count: usize,
    ) -> TranscriptWorkflowsProjection {
        let Some(store) = &self.store else {
            return unavailable();
        };
        if let Some(episode_id) = episode_id {
            return match store.transcript_workflow(episode_id) {
                Ok(record) => TranscriptWorkflowsProjection {
                    workflows: record.into_iter().map(workflow_projection).collect(),
                    has_more: false,
                    failure: None,
                },
                Err(_) => unavailable(),
            };
        }
        let page = match store.transcript_workflow_page(offset as u32, maximum_count as u16) {
            Ok(page) => page,
            Err(_) => return unavailable(),
        };
        TranscriptWorkflowsProjection {
            workflows: page.items.into_iter().map(workflow_projection).collect(),
            has_more: page.has_more,
            failure: None,
        }
    }
}

fn unavailable() -> TranscriptWorkflowsProjection {
    TranscriptWorkflowsProjection {
        workflows: Vec::new(),
        has_more: false,
        failure: Some(failure(CoreFailureCode::StorageUnavailable)),
    }
}

fn workflow_projection(record: TranscriptWorkflowRecord) -> TranscriptWorkflowProjection {
    let stage = stage(record.stage);
    TranscriptWorkflowProjection {
        episode_id: record.episode_id,
        workflow_id: record.request.workflow_id,
        source_revision: record.request.source_revision,
        origin: origin(&record.request.origin),
        provider: provider(&record.request.provider),
        model: record.request.model,
        stage,
        workflow_revision: record.workflow_revision,
        attempt: record.attempt,
        attempt_id: record.attempt_id,
        submission_fence_id: record.submission_fence_id,
        request_id: record.request_id,
        external_operation_present: record.external_operation_id.is_some(),
        not_before: record.not_before_ms.map(UnixTimestampMilliseconds::new),
        failure: record
            .failure_code
            .as_deref()
            .map(|code| TranscriptWorkflowFailure {
                code: failure_code(code),
                safe_detail: record.failure_detail,
                retryable: record.failure_retryable,
            }),
        updated_at: UnixTimestampMilliseconds::new(record.updated_at_ms),
        allowed_actions: transcript_allowed_actions(stage),
    }
}

pub(super) const fn stage(value: StoredTranscriptWorkflowStage) -> TranscriptWorkflowStage {
    match value {
        StoredTranscriptWorkflowStage::AwaitingPrerequisite => {
            TranscriptWorkflowStage::AwaitingPrerequisite
        }
        StoredTranscriptWorkflowStage::Requested => TranscriptWorkflowStage::Requested,
        StoredTranscriptWorkflowStage::PublisherRequested => {
            TranscriptWorkflowStage::PublisherRequested
        }
        StoredTranscriptWorkflowStage::SubmissionAuthorized => {
            TranscriptWorkflowStage::SubmissionAuthorized
        }
        StoredTranscriptWorkflowStage::ProviderAccepted => {
            TranscriptWorkflowStage::ProviderAccepted
        }
        StoredTranscriptWorkflowStage::CompletionObserved => {
            TranscriptWorkflowStage::CompletionObserved
        }
        StoredTranscriptWorkflowStage::TranscriptCommitted => {
            TranscriptWorkflowStage::TranscriptCommitted
        }
        StoredTranscriptWorkflowStage::EvidenceRequested => {
            TranscriptWorkflowStage::EvidenceRequested
        }
        StoredTranscriptWorkflowStage::RetryScheduled => TranscriptWorkflowStage::RetryScheduled,
        StoredTranscriptWorkflowStage::Blocked => TranscriptWorkflowStage::Blocked,
        StoredTranscriptWorkflowStage::Failed => TranscriptWorkflowStage::Failed,
        StoredTranscriptWorkflowStage::Cancelled => TranscriptWorkflowStage::Cancelled,
        StoredTranscriptWorkflowStage::Succeeded => TranscriptWorkflowStage::Succeeded,
    }
}

pub(super) fn provider(value: &str) -> TranscriptProvider {
    match value {
        "assembly-ai" => TranscriptProvider::AssemblyAi,
        "elevenlabs-scribe" => TranscriptProvider::ElevenLabsScribe,
        "openrouter-whisper" => TranscriptProvider::OpenRouterWhisper,
        "apple-speech" => TranscriptProvider::AppleSpeech,
        _ => TranscriptProvider::Unsupported { wire_code: 1 },
    }
}

fn origin(value: &str) -> TranscriptWorkflowOrigin {
    match value {
        "user" => TranscriptWorkflowOrigin::User,
        "automatic" => TranscriptWorkflowOrigin::Automatic,
        "playback" => TranscriptWorkflowOrigin::Playback,
        _ => TranscriptWorkflowOrigin::Unsupported { wire_code: 1 },
    }
}

pub(super) fn failure_code(value: &str) -> TranscriptWorkflowFailureCode {
    use TranscriptWorkflowFailureCode as Code;
    match value {
        "missing_credential" => Code::MissingCredential,
        "missing_local_audio" => Code::MissingLocalAudio,
        "invalid_request" => Code::InvalidRequest,
        "unsupported_provider" => Code::UnsupportedProvider,
        "publisher_unavailable" => Code::PublisherUnavailable,
        "offline" => Code::Offline,
        "rate_limited" => Code::RateLimited,
        "timed_out" => Code::TimedOut,
        "transport" => Code::Transport,
        "permission_denied" => Code::PermissionDenied,
        "provider_rejected" => Code::ProviderRejected,
        "provider_unavailable" => Code::ProviderUnavailable,
        "response_too_large" => Code::ResponseTooLarge,
        "invalid_response" => Code::InvalidResponse,
        "stale_input" => Code::StaleInput,
        "storage_unavailable" => Code::StorageUnavailable,
        "ambiguous_submission" => Code::AmbiguousSubmission,
        "provider_recovery_unavailable" => Code::ProviderRecoveryUnavailable,
        "retry_exhausted" => Code::RetryExhausted,
        "cancelled" => Code::Cancelled,
        _ => Code::Unsupported { wire_code: 1 },
    }
}
