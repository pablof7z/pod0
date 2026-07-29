use pod0_domain::{SpeakerEntityId, SpeakerId, TranscriptArtifactId};

use crate::StorageError;

pub const MAX_SPEAKER_DISPLAY_NAME_BYTES: usize = 1_024;

/// Who decided an assignment. The distinction is permanent authority, not
/// provenance trivia: diarization indices can permute within a provider
/// across runs, so a carried-forward link must stay visibly revisable while
/// a user's explicit naming can never be silently downgraded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpeakerAssignmentOrigin {
    /// The person using the app named this speaker.
    User,
    /// Carried forward or otherwise machine-decided; revisable.
    Inferred,
    /// Taken from publisher feed metadata.
    FeedMetadata,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredSpeakerEntity {
    pub speaker_entity_id: SpeakerEntityId,
    pub revision: u64,
    pub display_name: String,
    pub deleted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StoredSpeakerAssignment {
    pub artifact_id: TranscriptArtifactId,
    pub speaker_id: SpeakerId,
    pub speaker_entity_id: SpeakerEntityId,
    pub confidence: Option<f64>,
    pub origin: SpeakerAssignmentOrigin,
    pub decided_at_ms: i64,
}

pub(crate) fn encode_assignment_origin(origin: SpeakerAssignmentOrigin) -> i64 {
    match origin {
        SpeakerAssignmentOrigin::User => 1,
        SpeakerAssignmentOrigin::Inferred => 2,
        SpeakerAssignmentOrigin::FeedMetadata => 3,
    }
}

pub(crate) fn decode_assignment_origin(code: i64) -> Result<SpeakerAssignmentOrigin, StorageError> {
    match code {
        1 => Ok(SpeakerAssignmentOrigin::User),
        2 => Ok(SpeakerAssignmentOrigin::Inferred),
        3 => Ok(SpeakerAssignmentOrigin::FeedMetadata),
        // A row the schema CHECK should have rejected means the file was
        // written by something other than this kernel.
        _ => Err(StorageError::CorruptSchema {
            detail: "speaker assignment origin code is unsupported",
        }),
    }
}

pub(crate) fn validate_display_name(display_name: &str) -> Result<(), StorageError> {
    if display_name.trim().is_empty() || display_name.len() > MAX_SPEAKER_DISPLAY_NAME_BYTES {
        return Err(StorageError::InvalidSpeakerEntity);
    }
    Ok(())
}

pub(crate) fn validate_confidence(confidence: Option<f64>) -> Result<(), StorageError> {
    match confidence {
        Some(value) if !(0.0..=1.0).contains(&value) => Err(StorageError::InvalidSpeakerEntity),
        _ => Ok(()),
    }
}
