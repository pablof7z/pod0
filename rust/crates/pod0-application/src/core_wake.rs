use pod0_domain::{
    ChapterModelSubmissionFenceId, EpisodeId, FeedDiscoveryOccurrenceId, HostRequestId, PodcastId,
    TranscriptAttemptId, TranscriptSubmissionFenceId,
};

/// Durable product work that needs a native wake without exposing native
/// scheduler concepts to the product kernel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum CoreWakeReason {
    ModelChapterRetry {
        episode_id: EpisodeId,
        generation: u64,
        submission_fence_id: ChapterModelSubmissionFenceId,
    },
    ModelChapterFinalization {
        request_id: HostRequestId,
    },
    TranscriptProviderRecovery {
        episode_id: EpisodeId,
        attempt_id: TranscriptAttemptId,
        submission_fence_id: TranscriptSubmissionFenceId,
    },
    TranscriptRetry {
        episode_id: EpisodeId,
        attempt_id: TranscriptAttemptId,
        submission_fence_id: TranscriptSubmissionFenceId,
    },
    TranscriptFinalization {
        request_id: HostRequestId,
    },
    FeedDiscoveryNotificationRetry {
        occurrence_id: FeedDiscoveryOccurrenceId,
        episode_id: EpisodeId,
        attempt: u8,
    },
    FeedFetchRetry {
        podcast_id: PodcastId,
        attempt: u16,
    },
    Unsupported {
        wire_code: u32,
    },
}
