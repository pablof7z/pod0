use pod0_domain::{
    CancellationId, ChapterArtifactId, ChapterArtifactSource, ChapterModelSubmissionFenceId,
    CommandId, ContentDigest, EpisodeId, HostRequestId, StateRevision, TranscriptVersionId,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelChapterWorkflowState {
    AwaitingTranscript,
    AwaitingPublisher,
    Preserved,
    Requested,
    SubmissionAuthorized,
    ProviderAccepted,
    Ambiguous,
    CompletionObserved,
    RetryScheduled,
    Blocked,
    Failed,
    Cancelled,
    Succeeded,
}

impl ModelChapterWorkflowState {
    pub(crate) const fn wire(self) -> &'static str {
        match self {
            Self::AwaitingTranscript => "awaiting_transcript",
            Self::AwaitingPublisher => "awaiting_publisher",
            Self::Preserved => "preserved",
            Self::Requested => "requested",
            Self::SubmissionAuthorized => "submission_authorized",
            Self::ProviderAccepted => "provider_accepted",
            Self::Ambiguous => "ambiguous",
            Self::CompletionObserved => "completion_observed",
            Self::RetryScheduled => "retry_scheduled",
            Self::Blocked => "blocked",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Succeeded => "succeeded",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "awaiting_transcript" => Self::AwaitingTranscript,
            "awaiting_publisher" => Self::AwaitingPublisher,
            "preserved" => Self::Preserved,
            "requested" => Self::Requested,
            "submission_authorized" => Self::SubmissionAuthorized,
            "provider_accepted" => Self::ProviderAccepted,
            "ambiguous" => Self::Ambiguous,
            "completion_observed" => Self::CompletionObserved,
            "retry_scheduled" => Self::RetryScheduled,
            "blocked" => Self::Blocked,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            "succeeded" => Self::Succeeded,
            _ => return None,
        })
    }

    pub const fn may_have_submitted(self) -> bool {
        matches!(
            self,
            Self::SubmissionAuthorized
                | Self::ProviderAccepted
                | Self::Ambiguous
                | Self::CompletionObserved
                | Self::Succeeded
        )
    }

    pub const fn protects_active_attempt(self) -> bool {
        matches!(
            self,
            Self::SubmissionAuthorized
                | Self::ProviderAccepted
                | Self::Ambiguous
                | Self::CompletionObserved
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelChapterWorkflowMode {
    Generate,
    Enrich,
}

impl ModelChapterWorkflowMode {
    pub(crate) const fn wire(self) -> &'static str {
        match self {
            Self::Generate => "generate",
            Self::Enrich => "enrich",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "generate" => Some(Self::Generate),
            "enrich" => Some(Self::Enrich),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredModelChapterRequest {
    pub configured_model: String,
    pub mode: ModelChapterWorkflowMode,
    pub source_version: String,
    pub request_fingerprint: ContentDigest,
    pub requested_transcript_version_id: TranscriptVersionId,
    pub requested_transcript_digest: ContentDigest,
    pub selected_transcript_version_id: TranscriptVersionId,
    pub selected_transcript_digest: ContentDigest,
    pub expected_selection_revision: StateRevision,
    pub base_artifact_id: Option<ChapterArtifactId>,
    pub base_integrity_digest: Option<ContentDigest>,
    pub format_version: u32,
    pub policy_version: u32,
    pub provider: String,
    pub model: String,
    pub response_format_code: u32,
    pub maximum_completion_bytes: u64,
    pub duration_ms: Option<u64>,
    pub expected_artifact_source: ChapterArtifactSource,
    pub system_prompt: String,
    pub user_prompt: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelChapterDesiredPlan {
    AwaitingTranscript,
    AwaitingPublisher,
    PreserveAgentComposed {
        artifact_id: ChapterArtifactId,
        selection_revision: StateRevision,
    },
    Ready(Box<StoredModelChapterRequest>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelChapterWorkflowRecord {
    pub episode_id: EpisodeId,
    pub state: ModelChapterWorkflowState,
    pub desired_configured_model: String,
    pub active_request: Option<StoredModelChapterRequest>,
    pub replan_pending: bool,
    pub generation: u64,
    pub workflow_revision: StateRevision,
    pub attempt: u16,
    pub max_attempts: u16,
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub request_id: Option<HostRequestId>,
    pub submission_fence_id: Option<ChapterModelSubmissionFenceId>,
    pub issued_revision: StateRevision,
    pub deadline_at_ms: Option<i64>,
    pub not_before_ms: Option<i64>,
    pub submission_authorized_at_ms: Option<i64>,
    pub provider_operation_id: Option<String>,
    pub provider_status: Option<String>,
    pub selected_artifact_id: Option<ChapterArtifactId>,
    pub failure_code: Option<String>,
    pub failure_detail: Option<String>,
    pub may_have_submitted: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelChapterEnsureInput {
    pub episode_id: EpisodeId,
    pub configured_model: String,
    pub desired_plan: ModelChapterDesiredPlan,
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub issued_revision: StateRevision,
    pub now_ms: i64,
    pub request_deadline_ms: i64,
    pub max_attempts: u16,
    pub force_retry_from_revision: Option<StateRevision>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelChapterEnsureOutcome {
    Changed {
        record: ModelChapterWorkflowRecord,
        replaced: Option<Box<ModelChapterWorkflowRecord>>,
    },
    Existing(ModelChapterWorkflowRecord),
}
