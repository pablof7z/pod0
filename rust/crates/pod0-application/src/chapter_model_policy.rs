use pod0_domain::{
    ChapterArtifactInput, ChapterArtifactSource, ContentDigest, EpisodeId, PodcastId,
    StateRevision, TranscriptVersionId,
};

use crate::ChapterModelObservationMode;

pub const CHAPTER_MODEL_FORMAT_VERSION: u32 = 1;
pub const CHAPTER_MODEL_POLICY_VERSION: u32 = 1;
pub const CHAPTER_MODEL_POLICY_ID: &str = "chapter-prompt-v1";
pub const MAX_CHAPTER_MODEL_TRANSCRIPT_CHARACTERS: usize = 28_000;
pub const MAX_CHAPTER_MODEL_TRANSCRIPT_SEGMENTS: usize = 50_000;
pub const MAX_CHAPTER_MODEL_TRANSCRIPT_INPUT_BYTES: usize = 16 * 1_024 * 1_024;
pub const MAX_CHAPTER_MODEL_EPISODE_TEXT_BYTES: usize = 64 * 1_024;

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChapterModelDesiredStateInput {
    pub transcript_content_digest: ContentDigest,
    pub configured_model: String,
    pub selected_chapter_source: Option<ChapterArtifactSource>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ChapterModelDesiredStatePlan {
    Compile { input_version: String },
    PreserveAgentComposed,
    UnsupportedArtifact,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ChapterModelEpisodeInput {
    pub episode_id: EpisodeId,
    pub podcast_id: PodcastId,
    pub title: String,
    pub description: String,
    pub duration_seconds: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ChapterModelTranscriptSegmentInput {
    pub start_seconds: f64,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ChapterModelTranscriptInput {
    pub transcript_version_id: TranscriptVersionId,
    pub transcript_content_digest: ContentDigest,
    pub segments: Vec<ChapterModelTranscriptSegmentInput>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ChapterModelPlanInput {
    pub episode: ChapterModelEpisodeInput,
    pub requested_transcript_version_id: TranscriptVersionId,
    pub requested_transcript_content_digest: ContentDigest,
    pub selected_transcript: Option<ChapterModelTranscriptInput>,
    pub selected_chapter_artifact: Option<ChapterArtifactInput>,
    pub expected_chapter_selection_revision: StateRevision,
    pub configured_model: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ChapterModelResponseFormat {
    JsonObject,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PlannedChapterModelRequest {
    pub source_version: String,
    pub episode_id: EpisodeId,
    pub podcast_id: PodcastId,
    pub format_version: u32,
    pub requested_transcript_version_id: TranscriptVersionId,
    pub requested_transcript_content_digest: ContentDigest,
    pub selected_transcript_version_id: TranscriptVersionId,
    pub selected_transcript_content_digest: ContentDigest,
    pub policy_version: u32,
    pub provider: String,
    pub model: String,
    pub system_prompt: String,
    pub user_prompt: String,
    pub response_format: ChapterModelResponseFormat,
    pub maximum_completion_bytes: u64,
    pub duration_milliseconds: Option<u64>,
    pub mode: ChapterModelObservationMode,
    pub expected_artifact_source: ChapterArtifactSource,
    pub expected_chapter_selection_revision: StateRevision,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
#[allow(clippy::large_enum_variant)]
pub enum ChapterModelPlan {
    Ready { request: PlannedChapterModelRequest },
    EpisodeUnavailable,
    TranscriptUnavailable,
    StaleTranscript,
    PreserveAgentComposed,
    InvalidConfiguration,
    UnsupportedArtifact,
    InvalidInput,
    EmptyTranscript,
    InputTooLarge,
    CoreUnavailable,
}

#[must_use]
pub fn plan_chapter_model_desired_state(
    input: ChapterModelDesiredStateInput,
) -> ChapterModelDesiredStatePlan {
    crate::chapter_model_policy_source::desired_state(input, CHAPTER_MODEL_POLICY_ID)
}

#[must_use]
pub fn plan_chapter_model_request(input: ChapterModelPlanInput) -> ChapterModelPlan {
    crate::chapter_model_policy_source::request(input)
}
