use pod0_domain::{
    ClipEvidenceReference, ClipId, ClipRevision, ClipSource, ContentDigest, EpisodeId,
    EvidenceGenerationId, EvidenceSpanId, PodcastId, SpeakerId, TranscriptVersionId,
};

use crate::StorageError;

pub(crate) fn clip_id(bytes: &[u8]) -> Result<ClipId, StorageError> {
    Ok(ClipId::from_bytes(id(bytes, "clip identity")?))
}

pub(crate) fn episode_id(bytes: &[u8]) -> Result<EpisodeId, StorageError> {
    Ok(EpisodeId::from_bytes(id(bytes, "clip episode identity")?))
}

pub(crate) fn podcast_id(bytes: &[u8]) -> Result<PodcastId, StorageError> {
    Ok(PodcastId::from_bytes(id(bytes, "clip podcast identity")?))
}

pub(crate) fn speaker_id(bytes: Option<Vec<u8>>) -> Result<Option<SpeakerId>, StorageError> {
    bytes
        .map(|value| id(&value, "clip speaker identity").map(SpeakerId::from_bytes))
        .transpose()
}

pub(crate) fn clip_revision(value: i64) -> Result<ClipRevision, StorageError> {
    let value = u64::try_from(value).map_err(|_| corrupt("clip revision"))?;
    if value == 0 {
        return Err(corrupt("clip revision"));
    }
    Ok(ClipRevision::new(value))
}

pub(crate) const fn encode_source(value: ClipSource) -> (i64, Option<i64>) {
    match value {
        ClipSource::Touch => (1, None),
        ClipSource::Auto => (2, None),
        ClipSource::Headphone => (3, None),
        ClipSource::Carplay => (4, None),
        ClipSource::Watch => (5, None),
        ClipSource::Siri => (6, None),
        ClipSource::Agent => (7, None),
        ClipSource::Unsupported { wire_code } => (255, Some(wire_code as i64)),
    }
}

pub(crate) fn decode_source(code: i64, wire: Option<i64>) -> Result<ClipSource, StorageError> {
    match (code, wire) {
        (1, None) => Ok(ClipSource::Touch),
        (2, None) => Ok(ClipSource::Auto),
        (3, None) => Ok(ClipSource::Headphone),
        (4, None) => Ok(ClipSource::Carplay),
        (5, None) => Ok(ClipSource::Watch),
        (6, None) => Ok(ClipSource::Siri),
        (7, None) => Ok(ClipSource::Agent),
        (255, Some(wire)) => Ok(ClipSource::Unsupported {
            wire_code: u32::try_from(wire).map_err(|_| corrupt("clip source"))?,
        }),
        _ => Err(corrupt("clip source")),
    }
}

pub(crate) fn decode_evidence(
    generation: Option<Vec<u8>>,
    version: Option<Vec<u8>>,
    digest: Option<Vec<u8>>,
    span: Option<Vec<u8>>,
) -> Result<Option<ClipEvidenceReference>, StorageError> {
    match (generation, version, digest, span) {
        (None, None, None, None) => Ok(None),
        (Some(generation), Some(version), Some(digest), Some(span)) => {
            Ok(Some(ClipEvidenceReference {
                generation_id: EvidenceGenerationId::from_bytes(id(
                    &generation,
                    "clip evidence generation",
                )?),
                transcript_version_id: TranscriptVersionId::from_bytes(id(
                    &version,
                    "clip transcript version",
                )?),
                transcript_content_digest: ContentDigest::from_bytes(
                    digest
                        .try_into()
                        .map_err(|_| corrupt("clip evidence digest"))?,
                ),
                span_id: EvidenceSpanId::from_bytes(id(&span, "clip evidence span")?),
            }))
        }
        _ => Err(corrupt("clip evidence reference")),
    }
}

fn id(bytes: &[u8], detail: &'static str) -> Result<[u8; 16], StorageError> {
    bytes.try_into().map_err(|_| corrupt(detail))
}

fn corrupt(detail: &'static str) -> StorageError {
    StorageError::CorruptSchema { detail }
}
