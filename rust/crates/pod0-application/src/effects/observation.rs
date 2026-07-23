use pod0_domain::{
    AgentExecutionFenceId, AgentProposalId, AgentTurnId, CancellationId,
    ChapterModelSubmissionFenceId, ContentDigest, DownloadAttemptId, DownloadIntentId, EpisodeId,
    EvidenceGenerationId, HostRequestId, RecallQueryId, StateRevision, UnixTimestampMilliseconds,
};

use crate::{
    AgentCapabilityOutcome, AgentModelToolCallObservation, PlaybackLifecycleObservation,
    RecallEmbeddingVector, RecallRerankObservation, RecallSpanEmbeddingObservation,
};

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct HostObservationEnvelope {
    pub request_id: HostRequestId,
    pub cancellation_id: CancellationId,
    pub observed_request_revision: StateRevision,
    pub sequence_number: u64,
    pub observed_at: UnixTimestampMilliseconds,
    pub observation: HostObservation,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum HostObservation {
    FeedBytesFetched {
        bytes: Vec<u8>,
        entity_tag: Option<String>,
        last_modified: Option<String>,
        response_url: String,
        http_status: u16,
    },
    FeedNotModified {
        entity_tag: Option<String>,
        last_modified: Option<String>,
        response_url: String,
    },
    PlaybackObserved {
        value: PlaybackLifecycleObservation,
    },
    RecallQueryEmbedded {
        query_id: RecallQueryId,
        embedding: RecallEmbeddingVector,
    },
    RecallSpansEmbedded {
        episode_id: EpisodeId,
        generation_id: EvidenceGenerationId,
        embeddings: Vec<RecallSpanEmbeddingObservation>,
    },
    RecallCandidatesReranked {
        query_id: RecallQueryId,
        rankings: Vec<RecallRerankObservation>,
    },
    PublisherChaptersFetched {
        episode_id: EpisodeId,
        bytes: Vec<u8>,
        content_type: String,
        response_url: String,
        entity_tag: Option<String>,
        last_modified: Option<String>,
        http_status: u16,
    },
    ChapterModelProviderAccepted {
        episode_id: EpisodeId,
        generation: u64,
        submission_fence_id: ChapterModelSubmissionFenceId,
        update: crate::ChapterModelProviderUpdate,
    },
    ChapterModelCompleted {
        episode_id: EpisodeId,
        generation: u64,
        submission_fence_id: ChapterModelSubmissionFenceId,
        completion: crate::ChapterModelCompletionObservation,
    },
    ChapterModelFailed {
        episode_id: EpisodeId,
        generation: u64,
        submission_fence_id: ChapterModelSubmissionFenceId,
        code: crate::ChapterModelHostFailureCode,
        safe_detail: Option<String>,
        retry_after_milliseconds: Option<u64>,
    },
    DownloadAccepted {
        episode_id: EpisodeId,
        intent_id: DownloadIntentId,
        attempt_id: DownloadAttemptId,
        external_task_key: String,
        resume_key: Option<String>,
    },
    DownloadStaged {
        episode_id: EpisodeId,
        intent_id: DownloadIntentId,
        attempt_id: DownloadAttemptId,
        staged_file_path: String,
        byte_count: u64,
    },
    DownloadCancelled {
        episode_id: EpisodeId,
        intent_id: DownloadIntentId,
        attempt_id: DownloadAttemptId,
    },
    DownloadArtifactRemoved {
        episode_id: EpisodeId,
        artifact_key: String,
    },
    TranscriptCapabilityObserved {
        observation: crate::TranscriptCapabilityObservation,
    },
    ScheduledAgentExecutionObserved {
        observation: crate::ScheduledAgentExecutionObservation,
    },
    AgentModelCompleted {
        turn_id: AgentTurnId,
        model_fence_id: AgentExecutionFenceId,
        assistant_text: String,
        proposed_tool_call: Option<AgentModelToolCallObservation>,
    },
    AgentApprovalObserved {
        turn_id: AgentTurnId,
        proposal_id: AgentProposalId,
        proposal_digest: ContentDigest,
        approved: bool,
    },
    AgentCapabilityObserved {
        turn_id: AgentTurnId,
        proposal_id: AgentProposalId,
        execution_fence_id: AgentExecutionFenceId,
        outcome: AgentCapabilityOutcome,
    },
    CoreWakeReached {
        reason: crate::CoreWakeReason,
    },
    LegacyRecallIndexArtifactsRemoved {
        removed_file_count: u8,
    },
    Failed {
        code: HostFailureCode,
        safe_detail: Option<String>,
    },
    Cancelled,
    Unsupported {
        wire_code: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum HostFailureCode {
    Offline,
    TimedOut,
    PermissionDenied,
    InvalidResponse,
    ResponseTooLarge,
    MediaUnavailable,
    ProviderUnavailable,
    Unauthorized,
    IndexUnavailable,
    PlatformFailure,
    Unsupported { wire_code: u32 },
}
