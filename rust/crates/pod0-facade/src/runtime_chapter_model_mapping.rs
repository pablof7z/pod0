use pod0_application::{
    ChapterModelExecutionRequest, ChapterModelFailureEvidence, ChapterModelHostFailureCode,
    ChapterModelObservationMode, ChapterModelResponseFormat, ModelChapterWorkflowFailureCode,
    ModelChapterWorkflowStage, PlannedChapterModelRequest, chapter_model_request_fingerprint,
};
use pod0_domain::ChapterArtifact;
use pod0_storage::{
    ModelChapterWorkflowMode, ModelChapterWorkflowRecord, ModelChapterWorkflowState,
    StoredModelChapterRequest,
};

pub(super) fn stored_model_request(
    configured_model: &str,
    planned: PlannedChapterModelRequest,
) -> Option<StoredModelChapterRequest> {
    let request_fingerprint = chapter_model_request_fingerprint(&planned, configured_model).ok()?;
    let (mode, base_artifact_id, base_integrity_digest) = match &planned.mode {
        ChapterModelObservationMode::Generate => (ModelChapterWorkflowMode::Generate, None, None),
        ChapterModelObservationMode::Enrich { publisher_artifact } => {
            let base = ChapterArtifact::seal(publisher_artifact.clone()).ok()?;
            (
                ModelChapterWorkflowMode::Enrich,
                Some(base.artifact_id),
                Some(base.integrity_digest),
            )
        }
    };
    let response_format_code = match planned.response_format {
        ChapterModelResponseFormat::JsonObject => 1,
        ChapterModelResponseFormat::Unsupported { .. } => return None,
    };
    Some(StoredModelChapterRequest {
        configured_model: configured_model.to_owned(),
        mode,
        source_version: planned.source_version,
        request_fingerprint,
        requested_transcript_version_id: planned.requested_transcript_version_id,
        requested_transcript_digest: planned.requested_transcript_content_digest,
        selected_transcript_version_id: planned.selected_transcript_version_id,
        selected_transcript_digest: planned.selected_transcript_content_digest,
        expected_selection_revision: planned.expected_chapter_selection_revision,
        base_artifact_id,
        base_integrity_digest,
        format_version: planned.format_version,
        policy_version: planned.policy_version,
        provider: planned.provider,
        model: planned.model,
        response_format_code,
        maximum_completion_bytes: planned.maximum_completion_bytes,
        duration_ms: planned.duration_milliseconds,
        expected_artifact_source: planned.expected_artifact_source,
        system_prompt: planned.system_prompt,
        user_prompt: planned.user_prompt,
    })
}

pub(super) fn execution_request(
    request: &StoredModelChapterRequest,
) -> Option<ChapterModelExecutionRequest> {
    Some(ChapterModelExecutionRequest {
        provider: request.provider.clone(),
        model: request.model.clone(),
        system_prompt: request.system_prompt.clone(),
        user_prompt: request.user_prompt.clone(),
        response_format: match request.response_format_code {
            1 => ChapterModelResponseFormat::JsonObject,
            _ => return None,
        },
        maximum_completion_bytes: request.maximum_completion_bytes,
    })
}

pub(super) fn model_stage(state: ModelChapterWorkflowState) -> ModelChapterWorkflowStage {
    match state {
        ModelChapterWorkflowState::AwaitingTranscript => {
            ModelChapterWorkflowStage::AwaitingTranscript
        }
        ModelChapterWorkflowState::AwaitingPublisher => {
            ModelChapterWorkflowStage::AwaitingPublisher
        }
        ModelChapterWorkflowState::Preserved => ModelChapterWorkflowStage::Preserved,
        ModelChapterWorkflowState::Requested => ModelChapterWorkflowStage::Requested,
        ModelChapterWorkflowState::SubmissionAuthorized => {
            ModelChapterWorkflowStage::SubmissionAuthorized
        }
        ModelChapterWorkflowState::ProviderAccepted => ModelChapterWorkflowStage::ProviderAccepted,
        ModelChapterWorkflowState::Ambiguous => ModelChapterWorkflowStage::Ambiguous,
        ModelChapterWorkflowState::CompletionObserved => {
            ModelChapterWorkflowStage::CompletionObserved
        }
        ModelChapterWorkflowState::RetryScheduled => ModelChapterWorkflowStage::RetryScheduled,
        ModelChapterWorkflowState::Blocked => ModelChapterWorkflowStage::Blocked,
        ModelChapterWorkflowState::Failed => ModelChapterWorkflowStage::Failed,
        ModelChapterWorkflowState::Cancelled => ModelChapterWorkflowStage::Cancelled,
        ModelChapterWorkflowState::Succeeded => ModelChapterWorkflowStage::Succeeded,
    }
}

pub(super) fn failure_code(value: &str) -> ModelChapterWorkflowFailureCode {
    use ModelChapterWorkflowFailureCode as C;
    match value {
        "missing_credential" => C::MissingCredential,
        "invalid_request" => C::InvalidRequest,
        "rate_limited" => C::RateLimited,
        "provider_rejected" => C::ProviderRejected,
        "provider_unavailable" => C::ProviderUnavailable,
        "offline" => C::Offline,
        "timed_out" => C::TimedOut,
        "transport" => C::Transport,
        "response_too_large" => C::ResponseTooLarge,
        "invalid_response" => C::InvalidResponse,
        "qualification_rejected" => C::QualificationRejected,
        "stale_transcript" => C::StaleTranscript,
        "stale_publisher_base" => C::StalePublisherBase,
        "selection_changed" => C::SelectionChanged,
        "storage_unavailable" => C::StorageUnavailable,
        "ambiguous_submission" => C::AmbiguousSubmission,
        "provider_recovery_unavailable" => C::ProviderRecoveryUnavailable,
        "retry_exhausted" => C::RetryExhausted,
        "cancelled" => C::Cancelled,
        _ => C::Unsupported { wire_code: 1 },
    }
}

pub(super) fn failure_wire(code: ModelChapterWorkflowFailureCode) -> &'static str {
    use ModelChapterWorkflowFailureCode as C;
    match code {
        C::MissingCredential => "missing_credential",
        C::InvalidRequest => "invalid_request",
        C::RateLimited => "rate_limited",
        C::ProviderRejected => "provider_rejected",
        C::ProviderUnavailable => "provider_unavailable",
        C::Offline => "offline",
        C::TimedOut => "timed_out",
        C::Transport => "transport",
        C::ResponseTooLarge => "response_too_large",
        C::InvalidResponse => "invalid_response",
        C::QualificationRejected => "qualification_rejected",
        C::StaleTranscript => "stale_transcript",
        C::StalePublisherBase => "stale_publisher_base",
        C::SelectionChanged => "selection_changed",
        C::StorageUnavailable => "storage_unavailable",
        C::AmbiguousSubmission => "ambiguous_submission",
        C::ProviderRecoveryUnavailable => "provider_recovery_unavailable",
        C::RetryExhausted => "retry_exhausted",
        C::Cancelled => "cancelled",
        C::Unsupported { .. } => "unsupported",
    }
}

pub(super) fn host_failure_evidence(
    code: ChapterModelHostFailureCode,
) -> ChapterModelFailureEvidence {
    use ChapterModelHostFailureCode as H;
    match code {
        H::MissingCredential => ChapterModelFailureEvidence::MissingCredential,
        H::InvalidRequest => ChapterModelFailureEvidence::InvalidRequest,
        H::UnsupportedProvider => ChapterModelFailureEvidence::UnsupportedProvider,
        H::HttpResponse { status_code } => {
            ChapterModelFailureEvidence::HttpResponse { status_code }
        }
        H::Offline => ChapterModelFailureEvidence::Offline {
            submission_authorized: true,
        },
        H::TimedOut => ChapterModelFailureEvidence::TimedOut {
            submission_authorized: true,
        },
        H::Transport => ChapterModelFailureEvidence::Transport {
            submission_authorized: true,
        },
        H::ResponseTooLarge => ChapterModelFailureEvidence::ResponseTooLarge,
        H::InvalidResponse => ChapterModelFailureEvidence::InvalidResponse,
        H::ProviderRecoveryUnavailable => ChapterModelFailureEvidence::ProviderRecoveryUnavailable,
        H::Cancelled => ChapterModelFailureEvidence::Cancelled {
            submission_authorized: true,
        },
        H::Unsupported { wire_code } => ChapterModelFailureEvidence::Unsupported { wire_code },
    }
}

pub(super) fn request_is_current(
    record: &ModelChapterWorkflowRecord,
    request_id: pod0_domain::HostRequestId,
) -> bool {
    record.request_id == Some(request_id)
}
