use pod0_domain::{ChapterModelSubmissionFenceId, EpisodeId, HostRequestId};

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
    Unsupported {
        wire_code: u32,
    },
}
