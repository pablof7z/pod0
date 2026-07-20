use pod0_domain::{
    ChapterArtifactInput, ContentDigest, EpisodeId, PodcastId, TranscriptVersionId,
    UnixTimestampMilliseconds,
};

pub const CHAPTER_OBSERVATION_POLICY_VERSION: u32 = 1;
pub const MAX_PUBLISHER_CHAPTER_DOCUMENT_BYTES: usize = 2 * 1_024 * 1_024;
pub const MAX_MODEL_CHAPTER_COMPLETION_BYTES: usize = 1_024 * 1_024;
pub const MAX_AGENT_COMPOSED_CHAPTER_ITEMS: usize = 4_096;

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PublisherChapterObservation {
    pub episode_id: EpisodeId,
    pub podcast_id: PodcastId,
    pub resolved_source_url: String,
    pub content_type: String,
    pub payload_digest: ContentDigest,
    pub payload: Vec<u8>,
    pub generated_at: UnixTimestampMilliseconds,
    pub duration_milliseconds: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
// The enrichment base crosses UniFFI by value either way; boxing only the
// Rust discriminant would add indirection without reducing boundary bytes.
#[allow(clippy::large_enum_variant)]
pub enum ChapterModelObservationMode {
    Generate,
    Enrich {
        publisher_artifact: ChapterArtifactInput,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ModelChapterObservation {
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
    pub completion_digest: ContentDigest,
    pub completion: String,
    pub generated_at: UnixTimestampMilliseconds,
    pub duration_milliseconds: Option<u64>,
    pub mode: ChapterModelObservationMode,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct AgentComposedChapterItem {
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub title: String,
    pub summary: Option<String>,
    pub image_url: Option<String>,
    pub link_url: Option<String>,
    pub include_in_table_of_contents: bool,
    pub source_episode_id: Option<EpisodeId>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct AgentComposedChapterObservation {
    pub episode_id: EpisodeId,
    pub podcast_id: PodcastId,
    pub composition_revision: String,
    pub policy_version: u32,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub source_payload_digest: ContentDigest,
    pub generated_at: UnixTimestampMilliseconds,
    pub duration_milliseconds: Option<u64>,
    pub items: Vec<AgentComposedChapterItem>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ChapterObservationRejection {
    PayloadTooLarge,
    DigestMismatch,
    InvalidContentType,
    InvalidUrl,
    MalformedPayload,
    UnsupportedFormat { format_version: u32 },
    UnsupportedPolicy { policy_version: u32 },
    StaleTranscriptEvidence,
    InvalidProvenance,
    InvalidTimestamp,
    InvalidRange,
    InvalidBaseArtifact,
    NoUsableChapters,
    CollectionLimit,
    TextLimit,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
#[allow(clippy::large_enum_variant)]
pub enum ChapterObservationProjection {
    Qualified {
        artifact: ChapterArtifactInput,
        observation_fingerprint: ContentDigest,
    },
    Rejected {
        reason: ChapterObservationRejection,
    },
}

#[must_use]
pub fn qualify_publisher_chapter_observation(
    observation: PublisherChapterObservation,
) -> ChapterObservationProjection {
    project(crate::chapter_observation_publisher::qualify(observation))
}

#[must_use]
pub fn qualify_model_chapter_observation(
    observation: ModelChapterObservation,
) -> ChapterObservationProjection {
    project(crate::chapter_observation_model::qualify(observation))
}

#[must_use]
pub fn qualify_agent_composed_chapter_observation(
    observation: AgentComposedChapterObservation,
) -> ChapterObservationProjection {
    project(crate::chapter_observation_agent::qualify(observation))
}

pub(crate) type Qualification = Result<QualifiedObservation, ChapterObservationRejection>;

pub(crate) struct QualifiedObservation {
    pub(crate) artifact: ChapterArtifactInput,
    pub(crate) fingerprint: ContentDigest,
}

fn project(qualification: Qualification) -> ChapterObservationProjection {
    match qualification {
        Ok(value) => ChapterObservationProjection::Qualified {
            artifact: value.artifact,
            observation_fingerprint: value.fingerprint,
        },
        Err(reason) => ChapterObservationProjection::Rejected { reason },
    }
}
