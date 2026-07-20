use crate::chapter_artifact_hash::{
    ad_span_id, chapter_artifact_content_digest, chapter_artifact_digest, chapter_artifact_id,
    chapter_command_fingerprint, chapter_id,
};
use crate::chapter_artifact_validation::{
    canonical_ad_spans, canonical_chapters, validate_metadata,
};
use crate::{
    AdSpanId, ChapterArtifactId, ChapterId, ContentDigest, EpisodeId, PodcastId, StateRevision,
    TranscriptVersionId, UnixTimestampMilliseconds,
};

pub const CHAPTER_ARTIFACT_SCHEMA_VERSION: u32 = 1;
pub const MAX_CHAPTERS: usize = 4_096;
pub const MAX_AD_SPANS: usize = 4_096;
pub const MAX_CHAPTER_TITLE_BYTES: usize = 1_024;
pub const MAX_CHAPTER_SUMMARY_BYTES: usize = 16_384;
pub const MAX_CHAPTER_URL_BYTES: usize = 4_096;
pub const MAX_CHAPTER_ARTIFACT_BYTES: usize = 2 * 1_024 * 1_024;
pub const MAX_CHAPTER_MODEL_BYTES: usize = 256;
pub const MAX_CHAPTER_LEGACY_ORIGIN_BYTES: usize = 4_096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ChapterArtifactSource {
    Publisher,
    Generated,
    PublisherEnriched,
    AgentComposed,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ChapterAdKind {
    Preroll,
    Midroll,
    Postroll,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AdSpanEvaluation {
    NotEvaluated,
    Evaluated,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ChapterLegacySource {
    EpisodeAdjunct,
    WorkflowArtifactV0,
    WorkflowArtifactV1,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChapterLegacyProvenance {
    pub source: ChapterLegacySource,
    pub original_origin: Option<String>,
    pub generated_at_was_unknown: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChapterArtifactProvenance {
    pub source: ChapterArtifactSource,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub policy_version: u32,
    pub source_payload_digest: ContentDigest,
    pub transcript_version_id: Option<TranscriptVersionId>,
    pub transcript_content_digest: Option<ContentDigest>,
    /// Present only while preserving a pre-kernel artifact whose historical
    /// provider/model/transcript provenance was never recorded by Swift.
    pub legacy_import: Option<ChapterLegacyProvenance>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChapterInput {
    pub start_milliseconds: u64,
    pub end_milliseconds: Option<u64>,
    pub title: String,
    pub summary: Option<String>,
    pub image_url: Option<String>,
    pub link_url: Option<String>,
    pub include_in_table_of_contents: bool,
    pub source_episode_id: Option<EpisodeId>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AdSpanInput {
    pub start_milliseconds: u64,
    pub end_milliseconds: u64,
    pub kind: ChapterAdKind,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChapterArtifactInput {
    pub episode_id: EpisodeId,
    pub podcast_id: PodcastId,
    pub source_revision: String,
    pub provenance: ChapterArtifactProvenance,
    pub generated_at: UnixTimestampMilliseconds,
    pub duration_milliseconds: Option<u64>,
    pub chapters: Vec<ChapterInput>,
    pub ad_span_evaluation: AdSpanEvaluation,
    pub ad_spans: Vec<AdSpanInput>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChapterRecord {
    pub chapter_id: ChapterId,
    pub ordinal: u32,
    pub start_milliseconds: u64,
    pub end_milliseconds: Option<u64>,
    pub title: String,
    pub summary: Option<String>,
    pub image_url: Option<String>,
    pub link_url: Option<String>,
    pub include_in_table_of_contents: bool,
    pub source_episode_id: Option<EpisodeId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdSpanRecord {
    pub ad_span_id: AdSpanId,
    pub ordinal: u32,
    pub start_milliseconds: u64,
    pub end_milliseconds: u64,
    pub kind: ChapterAdKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChapterArtifact {
    pub schema_version: u32,
    pub artifact_id: ChapterArtifactId,
    pub content_digest: ContentDigest,
    pub integrity_digest: ContentDigest,
    pub episode_id: EpisodeId,
    pub podcast_id: PodcastId,
    pub source_revision: String,
    pub provenance: ChapterArtifactProvenance,
    pub generated_at: UnixTimestampMilliseconds,
    pub duration_milliseconds: Option<u64>,
    pub chapters: Vec<ChapterRecord>,
    pub ad_span_evaluation: AdSpanEvaluation,
    pub ad_spans: Vec<AdSpanRecord>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChapterArtifactError {
    InvalidMetadata,
    InvalidProvenance,
    InvalidChapter,
    ChaptersOutOfOrder,
    ChaptersOverlap,
    InvalidAdSpan,
    AdSpansOutOfOrder,
    AdSpansOverlap,
    TooManyChapters,
    TooManyAdSpans,
    TextLimit,
    ArtifactTooLarge,
    IdentityMismatch,
    UnsupportedSource { wire_code: u32 },
    UnsupportedAdKind { wire_code: u32 },
    UnsupportedAdEvaluation { wire_code: u32 },
}

impl std::fmt::Display for ChapterArtifactError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid chapter artifact: {self:?}")
    }
}

impl std::error::Error for ChapterArtifactError {}

impl ChapterArtifact {
    pub fn seal(input: ChapterArtifactInput) -> Result<Self, ChapterArtifactError> {
        validate_metadata(&input)?;
        let canonical_chapters = canonical_chapters(&input)?;
        let canonical_ad_spans = canonical_ad_spans(&input)?;
        let content_digest = chapter_artifact_content_digest(
            input.duration_milliseconds,
            &canonical_chapters,
            input.ad_span_evaluation,
            &canonical_ad_spans,
        );
        let artifact_id = chapter_artifact_id(
            input.episode_id,
            input.podcast_id,
            &input.source_revision,
            &input.provenance,
            content_digest,
        );
        let chapters = canonical_chapters
            .into_iter()
            .map(|chapter| ChapterRecord {
                chapter_id: chapter_id(artifact_id, &chapter),
                ordinal: chapter.ordinal,
                start_milliseconds: chapter.start_milliseconds,
                end_milliseconds: chapter.end_milliseconds,
                title: chapter.title,
                summary: chapter.summary,
                image_url: chapter.image_url,
                link_url: chapter.link_url,
                include_in_table_of_contents: chapter.include_in_table_of_contents,
                source_episode_id: chapter.source_episode_id,
            })
            .collect();
        let ad_spans = canonical_ad_spans
            .into_iter()
            .map(|span| AdSpanRecord {
                ad_span_id: ad_span_id(artifact_id, &span),
                ordinal: span.ordinal,
                start_milliseconds: span.start_milliseconds,
                end_milliseconds: span.end_milliseconds,
                kind: span.kind,
            })
            .collect();
        let mut artifact = Self {
            schema_version: CHAPTER_ARTIFACT_SCHEMA_VERSION,
            artifact_id,
            content_digest,
            integrity_digest: ContentDigest::default(),
            episode_id: input.episode_id,
            podcast_id: input.podcast_id,
            source_revision: input.source_revision,
            provenance: input.provenance,
            generated_at: input.generated_at,
            duration_milliseconds: input.duration_milliseconds,
            chapters,
            ad_span_evaluation: input.ad_span_evaluation,
            ad_spans,
        };
        artifact.integrity_digest = chapter_artifact_digest(&artifact);
        Ok(artifact)
    }

    pub fn verify_integrity(&self) -> Result<(), ChapterArtifactError> {
        let expected = Self::seal(self.as_input())?;
        if self == &expected {
            Ok(())
        } else {
            Err(ChapterArtifactError::IdentityMismatch)
        }
    }

    #[must_use]
    pub fn command_fingerprint(&self, expected_revision: StateRevision) -> ContentDigest {
        chapter_command_fingerprint(expected_revision, self)
    }

    #[must_use]
    pub fn as_input(&self) -> ChapterArtifactInput {
        ChapterArtifactInput {
            episode_id: self.episode_id,
            podcast_id: self.podcast_id,
            source_revision: self.source_revision.clone(),
            provenance: self.provenance.clone(),
            generated_at: self.generated_at,
            duration_milliseconds: self.duration_milliseconds,
            chapters: self.chapters.iter().map(ChapterRecord::as_input).collect(),
            ad_span_evaluation: self.ad_span_evaluation,
            ad_spans: self.ad_spans.iter().map(AdSpanRecord::as_input).collect(),
        }
    }
}

impl ChapterRecord {
    fn as_input(&self) -> ChapterInput {
        ChapterInput {
            start_milliseconds: self.start_milliseconds,
            end_milliseconds: self.end_milliseconds,
            title: self.title.clone(),
            summary: self.summary.clone(),
            image_url: self.image_url.clone(),
            link_url: self.link_url.clone(),
            include_in_table_of_contents: self.include_in_table_of_contents,
            source_episode_id: self.source_episode_id,
        }
    }
}

impl AdSpanRecord {
    fn as_input(&self) -> AdSpanInput {
        AdSpanInput {
            start_milliseconds: self.start_milliseconds,
            end_milliseconds: self.end_milliseconds,
            kind: self.kind,
        }
    }
}
