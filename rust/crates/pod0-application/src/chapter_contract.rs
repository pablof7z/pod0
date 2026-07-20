use pod0_domain::{
    AdSpanEvaluation, AdSpanId, ChapterAdKind, ChapterArtifact, ChapterArtifactError,
    ChapterArtifactId, ChapterArtifactInput, ChapterArtifactProvenance, ChapterId, CommandId,
    ContentDigest, EpisodeId, PodcastId, StateRevision, UnixTimestampMilliseconds,
};

use crate::{CoreFailure, OperationProjection};

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChapterContractRequest {
    pub command_id: CommandId,
    pub expected_selection_revision: StateRevision,
    pub artifact: ChapterArtifactInput,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChapterCommitReceipt {
    pub command_id: CommandId,
    pub artifact_id: ChapterArtifactId,
    pub content_digest: ContentDigest,
    pub integrity_digest: ContentDigest,
    pub command_fingerprint: ContentDigest,
    pub selection_revision: StateRevision,
    pub chapter_count: u32,
    pub ad_span_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ChapterProjectionScope {
    Summary,
    Chapters,
    Chapter { chapter_id: ChapterId },
    AdSpans,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChapterSummaryProjection {
    pub artifact_id: ChapterArtifactId,
    pub episode_id: EpisodeId,
    pub podcast_id: PodcastId,
    pub source_revision: String,
    pub provenance: ChapterArtifactProvenance,
    pub generated_at: UnixTimestampMilliseconds,
    pub duration_milliseconds: Option<u64>,
    pub content_digest: ContentDigest,
    pub integrity_digest: ContentDigest,
    pub selection_revision: StateRevision,
    pub chapter_count: u32,
    pub ad_span_evaluation: AdSpanEvaluation,
    pub ad_span_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChapterItemProjection {
    pub chapter_id: ChapterId,
    pub ordinal: u32,
    pub start_milliseconds: u64,
    pub explicit_end_milliseconds: Option<u64>,
    pub effective_end_milliseconds: Option<u64>,
    pub title: String,
    pub summary: Option<String>,
    pub image_url: Option<String>,
    pub link_url: Option<String>,
    pub include_in_table_of_contents: bool,
    pub source_episode_id: Option<EpisodeId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AdSpanProjection {
    pub ad_span_id: AdSpanId,
    pub ordinal: u32,
    pub start_milliseconds: u64,
    pub end_milliseconds: u64,
    pub kind: ChapterAdKind,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChapterArtifactProjection {
    pub scope: ChapterProjectionScope,
    pub summary: Option<ChapterSummaryProjection>,
    pub chapters: Vec<ChapterItemProjection>,
    pub ad_spans: Vec<AdSpanProjection>,
    pub operations: Vec<OperationProjection>,
    pub failure: Option<CoreFailure>,
    pub has_more: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
// The qualified payload is page-bounded and its collections are heap-backed.
// Boxing only the Rust enum variant would not reduce UniFFI serialization or
// native allocation, while it would complicate the generated value contract.
#[allow(clippy::large_enum_variant)]
pub enum ChapterContractProjection {
    Qualified {
        receipt: ChapterCommitReceipt,
        artifact: ChapterArtifactProjection,
    },
    Rejected {
        reason: ChapterContractRejection,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ChapterContractRejection {
    InvalidMetadata,
    InvalidProvenance,
    InvalidChapter,
    InvalidAdSpan,
    CollectionLimit,
    TextLimit,
    IdentityMismatch,
    RevisionExhausted,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChapterContractError {
    InvalidMetadata,
    InvalidProvenance,
    InvalidChapter,
    InvalidAdSpan,
    CollectionLimit,
    TextLimit,
    IdentityMismatch,
    RevisionExhausted,
    Unsupported { wire_code: u32 },
}

impl std::fmt::Display for ChapterContractError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid chapter contract: {self:?}")
    }
}

impl std::error::Error for ChapterContractError {}

/// Produces bounded, state-shaped contract evidence without mutating durable
/// state. Invalid input is represented by a rejection across UniFFI.
#[must_use]
pub fn project_chapter_contract(
    request: ChapterContractRequest,
    scope: ChapterProjectionScope,
    offset: u32,
    max_items: u16,
) -> ChapterContractProjection {
    match qualify_chapter_artifact(request) {
        Ok((receipt, artifact)) => ChapterContractProjection::Qualified {
            receipt,
            artifact: crate::project_chapter_artifact(
                &artifact,
                receipt.selection_revision,
                scope,
                usize::try_from(offset).unwrap_or(usize::MAX),
                usize::from(max_items),
            ),
        },
        Err(error) => ChapterContractProjection::Rejected {
            reason: error.into(),
        },
    }
}

pub fn qualify_chapter_commit(
    request: ChapterContractRequest,
) -> Result<ChapterCommitReceipt, ChapterContractError> {
    qualify_chapter_artifact(request).map(|(receipt, _)| receipt)
}

pub(crate) fn qualify_chapter_artifact(
    request: ChapterContractRequest,
) -> Result<(ChapterCommitReceipt, ChapterArtifact), ChapterContractError> {
    let selection_revision = StateRevision::new(
        request
            .expected_selection_revision
            .value
            .checked_add(1)
            .ok_or(ChapterContractError::RevisionExhausted)?,
    );
    let command_id = request.command_id;
    let expected_revision = request.expected_selection_revision;
    let artifact = ChapterArtifact::seal(request.artifact).map_err(ChapterContractError::from)?;
    let chapter_count = u32::try_from(artifact.chapters.len())
        .map_err(|_| ChapterContractError::CollectionLimit)?;
    let ad_span_count = u32::try_from(artifact.ad_spans.len())
        .map_err(|_| ChapterContractError::CollectionLimit)?;
    let receipt = ChapterCommitReceipt {
        command_id,
        artifact_id: artifact.artifact_id,
        content_digest: artifact.content_digest,
        integrity_digest: artifact.integrity_digest,
        command_fingerprint: artifact.command_fingerprint(expected_revision),
        selection_revision,
        chapter_count,
        ad_span_count,
    };
    Ok((receipt, artifact))
}

impl From<ChapterArtifactError> for ChapterContractError {
    fn from(value: ChapterArtifactError) -> Self {
        use ChapterArtifactError as Domain;
        match value {
            Domain::InvalidMetadata => Self::InvalidMetadata,
            Domain::InvalidProvenance => Self::InvalidProvenance,
            Domain::InvalidChapter | Domain::ChaptersOutOfOrder | Domain::ChaptersOverlap => {
                Self::InvalidChapter
            }
            Domain::InvalidAdSpan | Domain::AdSpansOutOfOrder | Domain::AdSpansOverlap => {
                Self::InvalidAdSpan
            }
            Domain::TooManyChapters | Domain::TooManyAdSpans => Self::CollectionLimit,
            Domain::TextLimit | Domain::ArtifactTooLarge => Self::TextLimit,
            Domain::IdentityMismatch => Self::IdentityMismatch,
            Domain::UnsupportedSource { wire_code }
            | Domain::UnsupportedAdKind { wire_code }
            | Domain::UnsupportedAdEvaluation { wire_code } => Self::Unsupported { wire_code },
        }
    }
}

impl From<ChapterContractError> for ChapterContractRejection {
    fn from(value: ChapterContractError) -> Self {
        match value {
            ChapterContractError::InvalidMetadata => Self::InvalidMetadata,
            ChapterContractError::InvalidProvenance => Self::InvalidProvenance,
            ChapterContractError::InvalidChapter => Self::InvalidChapter,
            ChapterContractError::InvalidAdSpan => Self::InvalidAdSpan,
            ChapterContractError::CollectionLimit => Self::CollectionLimit,
            ChapterContractError::TextLimit => Self::TextLimit,
            ChapterContractError::IdentityMismatch => Self::IdentityMismatch,
            ChapterContractError::RevisionExhausted => Self::RevisionExhausted,
            ChapterContractError::Unsupported { wire_code } => Self::Unsupported { wire_code },
        }
    }
}
