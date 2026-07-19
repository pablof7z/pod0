use pod0_domain::{
    ContentDigest, EpisodeId, EvidenceArtifactError, EvidenceGenerationId, EvidenceSpanId,
    PodcastId, SpeakerId, TranscriptSegmentId, TranscriptSource, TranscriptVersionId,
};

use crate::StorageError;
use crate::listening_db_codec::{decode_transcript_source, transcript_source};

macro_rules! id_decoder {
    ($name:ident, $type:ty, $detail:literal) => {
        pub(crate) fn $name(bytes: &[u8]) -> Result<$type, StorageError> {
            let bytes: [u8; 16] = bytes.try_into().map_err(|_| invalid($detail))?;
            Ok(<$type>::from_bytes(bytes))
        }
    };
}

id_decoder!(episode_id, EpisodeId, "episode ID");
id_decoder!(podcast_id, PodcastId, "podcast ID");
id_decoder!(generation_id, EvidenceGenerationId, "generation ID");
id_decoder!(span_id, EvidenceSpanId, "span ID");
id_decoder!(segment_id, TranscriptSegmentId, "segment ID");
id_decoder!(speaker_id, SpeakerId, "speaker ID");
id_decoder!(version_id, TranscriptVersionId, "transcript version ID");

pub(crate) fn optional_speaker_id(
    bytes: Option<Vec<u8>>,
) -> Result<Option<SpeakerId>, StorageError> {
    bytes.as_deref().map(speaker_id).transpose()
}

pub(crate) fn digest(bytes: &[u8]) -> Result<ContentDigest, StorageError> {
    let bytes: [u8; 32] = bytes.try_into().map_err(|_| invalid("content digest"))?;
    Ok(ContentDigest::from_bytes(bytes))
}

pub(crate) fn encode_source(source: &TranscriptSource) -> (i64, Option<i64>) {
    transcript_source(source)
}

pub(crate) fn decode_source(
    code: i64,
    wire: Option<i64>,
) -> Result<TranscriptSource, StorageError> {
    decode_transcript_source(code, wire).map_err(|_| invalid("transcript source"))
}

pub(crate) fn stored_u64(value: i64, detail: &'static str) -> Result<u64, StorageError> {
    u64::try_from(value).map_err(|_| invalid(detail))
}

pub(crate) fn stored_u32(value: i64, detail: &'static str) -> Result<u32, StorageError> {
    u32::try_from(value).map_err(|_| invalid(detail))
}

pub(crate) fn sqlite_i64(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::InvalidEvidenceArtifact)
}

pub(crate) fn artifact_error(error: EvidenceArtifactError) -> StorageError {
    match error {
        EvidenceArtifactError::NewerSchema { stored, supported } => {
            StorageError::NewerEvidenceSchema { stored, supported }
        }
        _ => StorageError::InvalidEvidenceArtifact,
    }
}

pub(crate) const fn invalid(_: &'static str) -> StorageError {
    StorageError::InvalidEvidenceArtifact
}
