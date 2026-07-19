use pod0_domain::{
    ContentDigest, EpisodeId, PodcastId, SpeakerId, StateRevision, TranscriptArtifactError,
    TranscriptArtifactId, TranscriptSegmentId, TranscriptVersionId,
};

use crate::StorageError;

macro_rules! id_decoder {
    ($name:ident, $type:ty, $detail:literal) => {
        pub(crate) fn $name(bytes: &[u8]) -> Result<$type, StorageError> {
            let bytes: [u8; 16] = bytes.try_into().map_err(|_| corrupt($detail))?;
            Ok(<$type>::from_bytes(bytes))
        }
    };
}

id_decoder!(
    artifact_id,
    TranscriptArtifactId,
    "transcript artifact identity"
);
id_decoder!(episode_id, EpisodeId, "transcript episode identity");
id_decoder!(podcast_id, PodcastId, "transcript podcast identity");
id_decoder!(
    segment_id,
    TranscriptSegmentId,
    "transcript segment identity"
);
id_decoder!(speaker_id, SpeakerId, "transcript speaker identity");
id_decoder!(
    version_id,
    TranscriptVersionId,
    "transcript version identity"
);

pub(crate) fn optional_artifact_id(
    value: Option<Vec<u8>>,
) -> Result<Option<TranscriptArtifactId>, StorageError> {
    value.as_deref().map(artifact_id).transpose()
}

pub(crate) fn optional_speaker_id(
    value: Option<Vec<u8>>,
) -> Result<Option<SpeakerId>, StorageError> {
    value.as_deref().map(speaker_id).transpose()
}

pub(crate) fn digest(value: &[u8]) -> Result<ContentDigest, StorageError> {
    Ok(ContentDigest::from_bytes(
        value
            .try_into()
            .map_err(|_| corrupt("transcript content digest"))?,
    ))
}

pub(crate) fn revision(value: i64) -> Result<StateRevision, StorageError> {
    let value = u64::try_from(value).map_err(|_| corrupt("transcript selection revision"))?;
    Ok(StateRevision::new(value))
}

pub(crate) fn stored_u64(value: i64, detail: &'static str) -> Result<u64, StorageError> {
    u64::try_from(value).map_err(|_| corrupt(detail))
}

pub(crate) fn stored_u32(value: i64, detail: &'static str) -> Result<u32, StorageError> {
    u32::try_from(value).map_err(|_| corrupt(detail))
}

pub(crate) fn sqlite_i64(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::InvalidTranscriptArtifact)
}

pub(crate) fn artifact_error(_: TranscriptArtifactError) -> StorageError {
    StorageError::InvalidTranscriptArtifact
}

pub(crate) const fn corrupt(detail: &'static str) -> StorageError {
    StorageError::CorruptSchema { detail }
}
