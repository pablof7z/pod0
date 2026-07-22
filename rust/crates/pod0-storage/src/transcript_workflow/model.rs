use pod0_domain::{
    CancellationId, CommandId, ContentDigest, EpisodeId, HostRequestId, StateRevision,
    TranscriptArtifactId, TranscriptArtifactInput, TranscriptAttemptId,
    TranscriptSubmissionFenceId, TranscriptVersionId, TranscriptWorkflowId,
};

use crate::TranscriptCommitStorageReceipt;

pub const MAX_TRANSCRIPT_WORKFLOW_PAGE_ITEMS: u16 = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoredTranscriptWorkflowStage {
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
}

impl StoredTranscriptWorkflowStage {
    pub(crate) const fn wire(self) -> &'static str {
        match self {
            Self::AwaitingPrerequisite => "awaiting_prerequisite",
            Self::Requested => "requested",
            Self::PublisherRequested => "publisher_requested",
            Self::SubmissionAuthorized => "submission_authorized",
            Self::ProviderAccepted => "provider_accepted",
            Self::CompletionObserved => "completion_observed",
            Self::TranscriptCommitted => "transcript_committed",
            Self::EvidenceRequested => "evidence_requested",
            Self::RetryScheduled => "retry_scheduled",
            Self::Blocked => "blocked",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Succeeded => "succeeded",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "awaiting_prerequisite" => Self::AwaitingPrerequisite,
            "requested" => Self::Requested,
            "publisher_requested" => Self::PublisherRequested,
            "submission_authorized" => Self::SubmissionAuthorized,
            "provider_accepted" => Self::ProviderAccepted,
            "completion_observed" => Self::CompletionObserved,
            "transcript_committed" => Self::TranscriptCommitted,
            "evidence_requested" => Self::EvidenceRequested,
            "retry_scheduled" => Self::RetryScheduled,
            "blocked" => Self::Blocked,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            "succeeded" => Self::Succeeded,
            _ => return None,
        })
    }

    pub const fn protects_submission(self) -> bool {
        matches!(
            self,
            Self::SubmissionAuthorized
                | Self::ProviderAccepted
                | Self::CompletionObserved
                | Self::TranscriptCommitted
                | Self::EvidenceRequested
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredTranscriptWorkflowRequest {
    pub workflow_id: TranscriptWorkflowId,
    pub source_revision: String,
    pub origin: String,
    pub provider: String,
    pub model: String,
    pub remote_audio_url: String,
    pub local_audio_url: Option<String>,
    pub publisher_transcript_url: Option<String>,
    pub publisher_mime_hint: Option<String>,
    pub publisher_first: bool,
    pub provider_fallback_enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreparedTranscriptAttempt {
    pub attempt: u16,
    pub attempt_id: TranscriptAttemptId,
    pub submission_fence_id: TranscriptSubmissionFenceId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptWorkflowRecord {
    pub episode_id: EpisodeId,
    pub request: StoredTranscriptWorkflowRequest,
    pub stage: StoredTranscriptWorkflowStage,
    pub workflow_revision: StateRevision,
    pub attempt: u16,
    pub max_attempts: u16,
    pub attempt_id: Option<TranscriptAttemptId>,
    pub submission_fence_id: Option<TranscriptSubmissionFenceId>,
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub request_id: Option<HostRequestId>,
    pub issued_revision: StateRevision,
    pub deadline_at_ms: Option<i64>,
    pub not_before_ms: Option<i64>,
    pub submission_authorized_at_ms: Option<i64>,
    pub external_operation_id: Option<String>,
    pub provider_status: Option<String>,
    pub completion_artifact_id: Option<TranscriptArtifactId>,
    pub committed_artifact_id: Option<TranscriptArtifactId>,
    pub committed_transcript_version_id: Option<TranscriptVersionId>,
    pub committed_content_digest: Option<ContentDigest>,
    pub expected_selection_revision: StateRevision,
    pub resulting_selection_revision: Option<StateRevision>,
    pub evidence_input_version: Option<String>,
    pub failure_code: Option<String>,
    pub failure_detail: Option<String>,
    pub failure_retryable: bool,
    pub may_have_submitted: bool,
    pub source_generation: Option<u64>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptWorkflowPage {
    pub items: Vec<TranscriptWorkflowRecord>,
    pub has_more: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptWorkflowEnsureInput {
    pub episode_id: EpisodeId,
    pub request: StoredTranscriptWorkflowRequest,
    pub stage: StoredTranscriptWorkflowStage,
    pub prepared_attempt: Option<PreparedTranscriptAttempt>,
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub request_id: Option<HostRequestId>,
    pub issued_revision: StateRevision,
    pub deadline_at_ms: Option<i64>,
    pub expected_selection_revision: StateRevision,
    pub max_attempts: u16,
    pub now_ms: i64,
    pub expected_workflow_revision: Option<StateRevision>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TranscriptWorkflowEnsureOutcome {
    Changed(TranscriptWorkflowRecord),
    Existing(TranscriptWorkflowRecord),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptSubmissionClaimInput {
    pub episode_id: EpisodeId,
    pub request_id: HostRequestId,
    pub attempt_id: TranscriptAttemptId,
    pub submission_fence_id: TranscriptSubmissionFenceId,
    pub cancellation_id: CancellationId,
    pub issued_revision: StateRevision,
    pub now_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TranscriptSubmissionClaim {
    Authorized(TranscriptWorkflowRecord),
    AlreadyClaimed(TranscriptWorkflowRecord),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptProviderAcceptedInput {
    pub episode_id: EpisodeId,
    pub request_id: HostRequestId,
    pub attempt_id: TranscriptAttemptId,
    pub submission_fence_id: TranscriptSubmissionFenceId,
    pub external_operation_id: String,
    pub provider_status: Option<String>,
    pub observed_at_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptProviderPendingInput {
    pub episode_id: EpisodeId,
    pub request_id: HostRequestId,
    pub attempt_id: TranscriptAttemptId,
    pub submission_fence_id: TranscriptSubmissionFenceId,
    pub provider_status: Option<String>,
    pub not_before_ms: i64,
    pub observed_at_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptCompletionInput {
    pub episode_id: EpisodeId,
    pub request_id: HostRequestId,
    pub attempt_id: Option<TranscriptAttemptId>,
    pub submission_fence_id: Option<TranscriptSubmissionFenceId>,
    pub external_operation_id: Option<String>,
    pub provider_status: Option<String>,
    pub artifact: TranscriptArtifactInput,
    pub observed_at_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptWorkflowCommitInput {
    pub episode_id: EpisodeId,
    pub request_id: HostRequestId,
    pub evidence_input_version: String,
    pub completed_at_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptWorkflowCommitReceipt {
    pub workflow: TranscriptWorkflowRecord,
    pub transcript: TranscriptCommitStorageReceipt,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TranscriptWorkflowFailureDisposition {
    Retry {
        attempt: PreparedTranscriptAttempt,
        request_id: HostRequestId,
        issued_revision: StateRevision,
        not_before_ms: i64,
        deadline_at_ms: i64,
        evidence_permits_resubmission: bool,
    },
    Replan,
    RecoverPersisted,
    Block,
    Fail,
    Ambiguous,
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptWorkflowFailureInput {
    pub episode_id: EpisodeId,
    pub request_id: HostRequestId,
    pub attempt_id: Option<TranscriptAttemptId>,
    pub submission_fence_id: Option<TranscriptSubmissionFenceId>,
    pub failure_code: String,
    pub failure_detail: Option<String>,
    pub retryable: bool,
    pub may_have_submitted: bool,
    pub disposition: TranscriptWorkflowFailureDisposition,
    pub observed_at_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptWorkflowRecoveryReport {
    pub dispatchable_requests: Vec<HostRequestId>,
    pub ambiguous_requests: Vec<HostRequestId>,
    pub provider_recoveries: Vec<HostRequestId>,
    pub completions_to_commit: Vec<HostRequestId>,
    pub evidence_requests: Vec<TranscriptWorkflowId>,
    pub has_more: bool,
}
