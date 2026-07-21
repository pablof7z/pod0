use pod0_domain::{
    CancellationId, ChapterArtifactId, EpisodeId, HostRequestId, StateRevision,
    UnixTimestampMilliseconds,
};
use sha2::{Digest as _, Sha256};
use url::Url;

pub const PUBLISHER_CHAPTER_WORKFLOW_POLICY_VERSION: u32 = 1;
pub const PUBLISHER_CHAPTER_MAX_ATTEMPTS: u16 = 5;
pub const PUBLISHER_CHAPTER_REQUEST_DEADLINE_MILLISECONDS: i64 = 30_000;
pub const MAX_ACTIVE_PUBLISHER_CHAPTER_REQUESTS: u16 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PublisherChapterWorkflowStage {
    Requested,
    RetryScheduled,
    Failed,
    Cancelled,
    Succeeded,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PublisherChapterWorkflowFailureCode {
    Offline,
    TimedOut,
    Transport,
    NotFound,
    ResponseTooLarge,
    InvalidResponse,
    InvalidDocument,
    SelectionChanged,
    StorageUnavailable,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PublisherChapterWorkflowFailure {
    pub code: PublisherChapterWorkflowFailureCode,
    pub safe_detail: Option<String>,
    pub retryable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PublisherChapterWorkflowProjection {
    pub episode_id: EpisodeId,
    pub source_version: String,
    pub stage: PublisherChapterWorkflowStage,
    pub workflow_revision: StateRevision,
    pub attempt: u16,
    pub max_attempts: u16,
    pub request_id: Option<HostRequestId>,
    pub cancellation_id: CancellationId,
    pub not_before: Option<UnixTimestampMilliseconds>,
    pub selected_artifact_id: Option<ChapterArtifactId>,
    pub failure: Option<PublisherChapterWorkflowFailure>,
    pub created_at: UnixTimestampMilliseconds,
    pub updated_at: UnixTimestampMilliseconds,
    pub can_retry: bool,
    pub can_cancel: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ModelChapterWorkflowFailure {
    pub code: crate::ModelChapterWorkflowFailureCode,
    pub safe_detail: Option<String>,
    pub retry: crate::ChapterModelRetryDisposition,
    pub may_have_submitted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ModelChapterWorkflowProjection {
    pub episode_id: EpisodeId,
    pub configured_model: String,
    pub mode: Option<crate::ModelChapterWorkflowMode>,
    pub source_version: Option<String>,
    pub stage: crate::ModelChapterWorkflowStage,
    pub workflow_revision: StateRevision,
    pub generation: u64,
    pub attempt: u16,
    pub max_attempts: u16,
    pub request_id: Option<HostRequestId>,
    pub cancellation_id: CancellationId,
    pub not_before: Option<UnixTimestampMilliseconds>,
    pub selected_artifact_id: Option<ChapterArtifactId>,
    pub failure: Option<ModelChapterWorkflowFailure>,
    pub replan_pending: bool,
    pub may_have_submitted: bool,
    pub created_at: UnixTimestampMilliseconds,
    pub updated_at: UnixTimestampMilliseconds,
    pub allowed_actions: crate::ModelChapterWorkflowAllowedActions,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChapterWorkflowsProjection {
    pub publisher: Vec<PublisherChapterWorkflowProjection>,
    pub model: Vec<ModelChapterWorkflowProjection>,
    pub has_more: bool,
    pub failure: Option<crate::CoreFailure>,
}

impl ChapterWorkflowsProjection {
    pub fn enforce_bounds(&mut self, offset: usize, requested_items: usize) {
        let limit = requested_items.clamp(1, usize::from(crate::MAX_PROJECTION_ITEMS));
        let count = self.publisher.len();
        self.publisher = self.publisher.drain(..).skip(offset).take(limit).collect();
        self.has_more |= count > offset.saturating_add(self.publisher.len());
        let model_count = self.model.len();
        self.model = self.model.drain(..).skip(offset).take(limit).collect();
        self.has_more |= model_count > offset.saturating_add(self.model.len());
    }
}

#[must_use]
pub fn publisher_chapter_source_version(source_url: &str) -> Option<String> {
    let parsed = Url::parse(source_url).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return None;
    }
    let normalized = crate::normalize_media_url(source_url)?;
    let mut hash = Sha256::new();
    hash.update(normalized.as_bytes());
    hash.update([0x1f]);
    hash.update(b"podcasting2-chapters-v1");
    Some(format!("{:x}", hash.finalize()))
}

#[must_use]
pub fn publisher_chapter_retry_delay_milliseconds(attempt: u16) -> i64 {
    let exponent = u32::from(attempt.saturating_sub(1).min(8));
    30_000_i64
        .saturating_mul(2_i64.saturating_pow(exponent))
        .min(3_600_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_version_matches_the_legacy_swift_contract() {
        assert_eq!(
            publisher_chapter_source_version("https://example.com/chapters.json").as_deref(),
            Some("8bc2da78ae23643cf233b7c2cd4c3d51986767aa9b37971f7cbaedc4efe0c14a")
        );
        assert!(publisher_chapter_source_version("file:///tmp/chapters.json").is_none());
    }

    #[test]
    fn retry_delay_is_bounded_and_deterministic() {
        assert_eq!(publisher_chapter_retry_delay_milliseconds(1), 30_000);
        assert_eq!(publisher_chapter_retry_delay_milliseconds(2), 60_000);
        assert_eq!(publisher_chapter_retry_delay_milliseconds(20), 3_600_000);
    }
}
