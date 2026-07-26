use pod0_domain::{
    ContentDigest, EpisodeId, HostRequestId, StateRevision, TranscriptAttemptId,
    TranscriptSubmissionFenceId, TranscriptVersionId, TranscriptWorkflowId,
    UnixTimestampMilliseconds,
};

pub const TRANSCRIPT_WORKFLOW_POLICY_VERSION: u32 = 1;
pub const TRANSCRIPT_WORKFLOW_MAX_ATTEMPTS: u16 = 8;
pub const TRANSCRIPT_RETRY_BASE_MILLISECONDS: i64 = 5_000;
pub const TRANSCRIPT_RETRY_MAX_MILLISECONDS: i64 = 3_600_000;
pub const TRANSCRIPT_HOST_REQUEST_DEADLINE_MILLISECONDS: i64 = 120_000;
pub const MAX_ACTIVE_TRANSCRIPT_WORKFLOWS: u16 = 200;
pub const MAX_TRANSCRIPT_MODEL_BYTES: usize = 256;
pub const MAX_TRANSCRIPT_EXTERNAL_ID_BYTES: usize = 1_024;
pub const MAX_TRANSCRIPT_PROVIDER_STATUS_BYTES: usize = 1_024;
pub const MAX_TRANSCRIPT_SAFE_DETAIL_BYTES: usize = 1_024;
pub const MAX_TRANSCRIPT_CAPABILITY_RESPONSE_BYTES: u64 = 32 * 1_024 * 1_024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum TranscriptWorkflowOrigin {
    User,
    Automatic,
    Playback,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum TranscriptProvider {
    AssemblyAi,
    ElevenLabsScribe,
    OpenRouterWhisper,
    AppleSpeech,
    Unsupported { wire_code: u32 },
}

impl TranscriptProvider {
    #[must_use]
    pub const fn requires_credential(self) -> bool {
        matches!(
            self,
            Self::AssemblyAi | Self::ElevenLabsScribe | Self::OpenRouterWhisper
        )
    }

    #[must_use]
    pub const fn requires_local_audio(self) -> bool {
        matches!(self, Self::AppleSpeech)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct CommittedTranscriptGeneration {
    pub source_revision: String,
    pub transcript_version_id: TranscriptVersionId,
    pub content_digest: ContentDigest,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TranscriptWorkflowPlanInput {
    pub episode_id: EpisodeId,
    pub source_revision: String,
    pub committed_transcript: Option<CommittedTranscriptGeneration>,
    pub selected_evidence_input_version: Option<String>,
    pub origin: TranscriptWorkflowOrigin,
    pub configured_provider: TranscriptProvider,
    pub configured_model: String,
    pub remote_audio_url: String,
    pub local_audio_url: Option<String>,
    pub publisher_transcript_url: Option<String>,
    pub publisher_mime_hint: Option<String>,
    pub auto_publisher_enabled: bool,
    pub auto_provider_enabled: bool,
    pub credential_available: bool,
    pub embedding_space_id: String,
}

/// Platform facts and user-selected provider configuration needed by the
/// shared kernel to plan a transcript workflow. Durable episode, transcript,
/// evidence, retry, and fallback state are intentionally not supplied by the
/// native application.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TranscriptWorkflowConfiguration {
    pub provider: TranscriptProvider,
    pub model: String,
    pub local_audio_url: Option<String>,
    pub credential_available: bool,
    pub auto_publisher_enabled: bool,
    pub auto_provider_enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TranscriptWorkflowRequest {
    pub workflow_id: TranscriptWorkflowId,
    pub episode_id: EpisodeId,
    pub source_revision: String,
    pub origin: TranscriptWorkflowOrigin,
    pub provider: TranscriptProvider,
    pub model: String,
    pub remote_audio_url: String,
    pub local_audio_url: Option<String>,
    pub publisher_transcript_url: Option<String>,
    pub publisher_mime_hint: Option<String>,
    pub publisher_first: bool,
    pub provider_fallback_enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum TranscriptGenerationDecision {
    NotRequested,
    Current,
    AwaitingCredential { provider: TranscriptProvider },
    AwaitingLocalAudio,
    Ensure,
    Blocked { code: TranscriptWorkflowFailureCode },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum TranscriptEvidenceDecision {
    AwaitingTranscript,
    Current,
    Ensure { input_version: String },
    Blocked { code: TranscriptWorkflowFailureCode },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TranscriptWorkflowPlan {
    pub generation: TranscriptGenerationDecision,
    pub request: Option<TranscriptWorkflowRequest>,
    pub evidence: TranscriptEvidenceDecision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum TranscriptWorkflowStage {
    AwaitingPrerequisite,
    Requested,
    PublisherRequested,
    SubmissionAuthorized,
    ProviderAccepted,
    CompletionObserved,
    TranscriptCommitted,
    EvidenceRequested,
    RetryScheduled,
    Blocked,
    Failed,
    Cancelled,
    Succeeded,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum TranscriptWorkflowFailureCode {
    MissingCredential,
    MissingLocalAudio,
    InvalidRequest,
    UnsupportedProvider,
    PublisherUnavailable,
    Offline,
    RateLimited,
    TimedOut,
    Transport,
    PermissionDenied,
    ProviderRejected,
    ProviderUnavailable,
    ResponseTooLarge,
    InvalidResponse,
    StaleInput,
    StorageUnavailable,
    AmbiguousSubmission,
    ProviderRecoveryUnavailable,
    RetryExhausted,
    Cancelled,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum TranscriptRetryDisposition {
    AutomaticRequest,
    Replan,
    RecoverPersisted,
    ExplicitOnly,
    Never,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TranscriptFailureClassification {
    pub code: TranscriptWorkflowFailureCode,
    pub retry: TranscriptRetryDisposition,
    pub may_have_submitted: bool,
    pub resubmission_is_safe: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum TranscriptFailureEvidence {
    MissingCredential,
    MissingLocalAudio,
    InvalidRequest,
    UnsupportedProvider,
    PublisherUnavailable,
    Offline {
        submission_authorized: bool,
        provider_accepted: bool,
    },
    RateLimited {
        submission_authorized: bool,
        provider_accepted: bool,
    },
    TimedOut {
        submission_authorized: bool,
        provider_accepted: bool,
    },
    Transport {
        submission_authorized: bool,
        provider_accepted: bool,
    },
    PermissionDenied,
    ProviderRejected,
    ProviderUnavailable {
        submission_authorized: bool,
        provider_accepted: bool,
    },
    ResponseTooLarge,
    InvalidResponse,
    StaleInput,
    StorageUnavailable {
        submission_authorized: bool,
        provider_accepted: bool,
    },
    ProviderRecoveryUnavailable,
    RetryExhausted {
        may_have_submitted: bool,
    },
    Cancelled {
        submission_authorized: bool,
        provider_accepted: bool,
    },
    Unsupported {
        wire_code: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TranscriptWorkflowAllowedActions {
    pub can_retry: bool,
    pub can_cancel: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TranscriptWorkflowFailure {
    pub code: TranscriptWorkflowFailureCode,
    pub safe_detail: Option<String>,
    pub retryable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TranscriptWorkflowProjection {
    pub episode_id: EpisodeId,
    pub workflow_id: TranscriptWorkflowId,
    pub source_revision: String,
    pub origin: TranscriptWorkflowOrigin,
    pub provider: TranscriptProvider,
    pub model: String,
    pub stage: TranscriptWorkflowStage,
    pub workflow_revision: StateRevision,
    pub attempt: u16,
    pub attempt_id: Option<TranscriptAttemptId>,
    pub submission_fence_id: Option<TranscriptSubmissionFenceId>,
    pub request_id: Option<HostRequestId>,
    pub external_operation_present: bool,
    pub not_before: Option<UnixTimestampMilliseconds>,
    pub failure: Option<TranscriptWorkflowFailure>,
    pub updated_at: UnixTimestampMilliseconds,
    pub allowed_actions: TranscriptWorkflowAllowedActions,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TranscriptWorkflowsProjection {
    pub workflows: Vec<TranscriptWorkflowProjection>,
    pub has_more: bool,
    pub failure: Option<crate::CoreFailure>,
}

impl TranscriptWorkflowsProjection {
    pub fn enforce_bounds(&mut self, offset: usize, requested_items: usize) {
        let limit = requested_items.clamp(
            1,
            usize::from(MAX_ACTIVE_TRANSCRIPT_WORKFLOWS.min(crate::MAX_PROJECTION_ITEMS)),
        );
        let count = self.workflows.len();
        self.workflows = self.workflows.drain(..).skip(offset).take(limit).collect();
        self.has_more |= count > offset.saturating_add(self.workflows.len());
    }
}
