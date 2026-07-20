use pod0_domain::{
    AdSpanEvaluation, AdSpanId, ChapterAdKind, ChapterArtifactId, ChapterArtifactSource, ChapterId,
    ChapterLegacySource, ContentDigest, EpisodeId, PodcastId, TranscriptVersionId,
};

use crate::StorageError;

macro_rules! id_decoder {
    ($name:ident, $type:ty, $detail:literal) => {
        pub(crate) fn $name(bytes: &[u8]) -> Result<$type, StorageError> {
            let value: [u8; 16] = bytes.try_into().map_err(|_| corrupt($detail))?;
            Ok(<$type>::from_bytes(value))
        }
    };
}

id_decoder!(artifact_id, ChapterArtifactId, "chapter artifact identity");
id_decoder!(chapter_id, ChapterId, "chapter identity");
id_decoder!(ad_span_id, AdSpanId, "ad span identity");
id_decoder!(episode_id, EpisodeId, "chapter episode identity");
id_decoder!(podcast_id, PodcastId, "chapter podcast identity");
id_decoder!(
    transcript_version_id,
    TranscriptVersionId,
    "chapter transcript version identity"
);

pub(crate) fn digest(bytes: &[u8]) -> Result<ContentDigest, StorageError> {
    let value: [u8; 32] = bytes
        .try_into()
        .map_err(|_| corrupt("chapter content digest"))?;
    Ok(ContentDigest::from_bytes(value))
}

pub(crate) const fn source_code(value: ChapterArtifactSource) -> Result<i64, StorageError> {
    match value {
        ChapterArtifactSource::Publisher => Ok(1),
        ChapterArtifactSource::Generated => Ok(2),
        ChapterArtifactSource::PublisherEnriched => Ok(3),
        ChapterArtifactSource::AgentComposed => Ok(4),
        ChapterArtifactSource::Unsupported { .. } => Err(StorageError::InvalidChapterArtifact),
    }
}

pub(crate) const fn source(value: i64) -> Result<ChapterArtifactSource, StorageError> {
    match value {
        1 => Ok(ChapterArtifactSource::Publisher),
        2 => Ok(ChapterArtifactSource::Generated),
        3 => Ok(ChapterArtifactSource::PublisherEnriched),
        4 => Ok(ChapterArtifactSource::AgentComposed),
        _ => Err(corrupt("chapter source code")),
    }
}

pub(crate) const fn legacy_source_code(value: ChapterLegacySource) -> Result<i64, StorageError> {
    match value {
        ChapterLegacySource::EpisodeAdjunct => Ok(1),
        ChapterLegacySource::WorkflowArtifactV0 => Ok(2),
        ChapterLegacySource::WorkflowArtifactV1 => Ok(3),
        ChapterLegacySource::Unsupported { .. } => Err(StorageError::InvalidChapterArtifact),
    }
}

pub(crate) const fn legacy_source(value: i64) -> Result<ChapterLegacySource, StorageError> {
    match value {
        1 => Ok(ChapterLegacySource::EpisodeAdjunct),
        2 => Ok(ChapterLegacySource::WorkflowArtifactV0),
        3 => Ok(ChapterLegacySource::WorkflowArtifactV1),
        _ => Err(corrupt("chapter legacy source code")),
    }
}

pub(crate) const fn evaluation_code(value: AdSpanEvaluation) -> Result<i64, StorageError> {
    match value {
        AdSpanEvaluation::NotEvaluated => Ok(1),
        AdSpanEvaluation::Evaluated => Ok(2),
        AdSpanEvaluation::Unsupported { .. } => Err(StorageError::InvalidChapterArtifact),
    }
}

pub(crate) const fn evaluation(value: i64) -> Result<AdSpanEvaluation, StorageError> {
    match value {
        1 => Ok(AdSpanEvaluation::NotEvaluated),
        2 => Ok(AdSpanEvaluation::Evaluated),
        _ => Err(corrupt("chapter ad evaluation code")),
    }
}

pub(crate) const fn ad_kind_code(value: ChapterAdKind) -> Result<i64, StorageError> {
    match value {
        ChapterAdKind::Preroll => Ok(1),
        ChapterAdKind::Midroll => Ok(2),
        ChapterAdKind::Postroll => Ok(3),
        ChapterAdKind::Unsupported { .. } => Err(StorageError::InvalidChapterArtifact),
    }
}

pub(crate) const fn ad_kind(value: i64) -> Result<ChapterAdKind, StorageError> {
    match value {
        1 => Ok(ChapterAdKind::Preroll),
        2 => Ok(ChapterAdKind::Midroll),
        3 => Ok(ChapterAdKind::Postroll),
        _ => Err(corrupt("chapter ad kind code")),
    }
}

pub(crate) fn sqlite_i64(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::InvalidChapterArtifact)
}

pub(crate) fn stored_u64(value: i64, detail: &'static str) -> Result<u64, StorageError> {
    u64::try_from(value).map_err(|_| corrupt(detail))
}

pub(crate) fn stored_u32(value: i64, detail: &'static str) -> Result<u32, StorageError> {
    u32::try_from(value).map_err(|_| corrupt(detail))
}

pub(crate) const fn corrupt(detail: &'static str) -> StorageError {
    StorageError::CorruptSchema { detail }
}
