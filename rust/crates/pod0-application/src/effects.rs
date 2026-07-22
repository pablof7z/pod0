use pod0_domain::{
    CancellationId, ChapterModelSubmissionFenceId, CommandId, DownloadAttemptId, DownloadIntentId,
    EpisodeId, EvidenceGenerationId, HostRequestId, PlaybackRatePermille, PlaybackSeekReason,
    RecallEmbeddingProvider, RecallQueryId, RecallRerankProvider, StateRevision,
    UnixTimestampMilliseconds,
};

use crate::{
    RecallEmbeddingInput, RecallEmbeddingVector, RecallRerankDocument, RecallRerankObservation,
    RecallSpanEmbeddingObservation,
};

mod domain_event;
mod playback;

pub use domain_event::*;
pub use playback::*;

pub const MAX_FEED_RESPONSE_BYTES: u64 = 8 * 1_024 * 1_024;
pub const MIN_PLAYBACK_OBSERVATION_INTERVAL_MILLISECONDS: u32 = 500;
pub const MAX_PLAYBACK_OBSERVATION_INTERVAL_MILLISECONDS: u32 = 5_000;

#[must_use]
pub fn bounded_host_request_count(requested: u16) -> usize {
    usize::from(requested.clamp(1, crate::MAX_HOST_REQUEST_BATCH))
}

#[must_use]
pub fn bounded_playback_observation_interval(requested_milliseconds: u32) -> u32 {
    requested_milliseconds.clamp(
        MIN_PLAYBACK_OBSERVATION_INTERVAL_MILLISECONDS,
        MAX_PLAYBACK_OBSERVATION_INTERVAL_MILLISECONDS,
    )
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct HostRequestEnvelope {
    pub request_id: HostRequestId,
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub issued_revision: StateRevision,
    pub deadline_at: Option<UnixTimestampMilliseconds>,
    pub request: HostRequest,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum HostRequest {
    FetchFeed {
        feed_url: String,
        entity_tag: Option<String>,
        last_modified: Option<String>,
        maximum_response_bytes: u64,
    },
    LoadMedia {
        episode_id: EpisodeId,
        audio_url: String,
        start_position_milliseconds: u64,
    },
    Play {
        episode_id: EpisodeId,
        transition_cue: PlaybackTransitionCue,
    },
    Pause {
        episode_id: EpisodeId,
    },
    Seek {
        episode_id: EpisodeId,
        position_milliseconds: u64,
        reason: PlaybackSeekReason,
        chapter_context: Option<crate::ChapterPlaybackContext>,
    },
    SetRate {
        episode_id: EpisodeId,
        rate: PlaybackRatePermille,
    },
    ArmNativeTimer {
        episode_id: EpisodeId,
        mode: NativeTimerMode,
    },
    CancelNativeTimer {
        episode_id: EpisodeId,
    },
    ObservePlayback {
        episode_id: Option<EpisodeId>,
        minimum_interval_milliseconds: u32,
    },
    StopPlayback {
        episode_id: EpisodeId,
    },
    EmbedRecallQuery {
        query_id: RecallQueryId,
        provider: RecallEmbeddingProvider,
        model: String,
        text: String,
        maximum_dimensions: u16,
    },
    EmbedRecallSpans {
        episode_id: EpisodeId,
        generation_id: EvidenceGenerationId,
        provider: RecallEmbeddingProvider,
        model: String,
        spans: Vec<RecallEmbeddingInput>,
        maximum_dimensions: u16,
    },
    RerankRecallCandidates {
        query_id: RecallQueryId,
        provider: RecallRerankProvider,
        model: String,
        query: String,
        candidates: Vec<RecallRerankDocument>,
    },
    FetchPublisherChapters {
        episode_id: EpisodeId,
        source_url: String,
        not_before: Option<UnixTimestampMilliseconds>,
        maximum_response_bytes: u64,
    },
    ExecuteChapterModel {
        episode_id: EpisodeId,
        generation: u64,
        submission_fence_id: ChapterModelSubmissionFenceId,
        execution: crate::ChapterModelExecutionRequest,
    },
    RecoverChapterModelOperation {
        episode_id: EpisodeId,
        generation: u64,
        submission_fence_id: ChapterModelSubmissionFenceId,
        provider: String,
        model: String,
        provider_operation_id: String,
        provider_status: Option<String>,
        maximum_completion_bytes: u64,
    },
    StartEpisodeDownload {
        episode_id: EpisodeId,
        intent_id: DownloadIntentId,
        attempt_id: DownloadAttemptId,
        input_version: String,
        enclosure_url: String,
        resume_key: Option<String>,
    },
    CancelEpisodeDownload {
        episode_id: EpisodeId,
        intent_id: DownloadIntentId,
        attempt_id: DownloadAttemptId,
        external_task_key: Option<String>,
    },
    RemoveEpisodeDownloadArtifact {
        episode_id: EpisodeId,
        artifact_key: String,
    },
    ScheduleCoreWake {
        wake_at: UnixTimestampMilliseconds,
        reason: crate::CoreWakeReason,
    },
    RemoveLegacyRecallIndexArtifacts,
    Unsupported {
        wire_code: u32,
    },
}

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
