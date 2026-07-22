use pod0_domain::{ContentDigest, StateRevision};

use super::model::StoredTranscriptWorkflowRequest;
use crate::StorageError;

pub(super) fn validate_request(
    request: &StoredTranscriptWorkflowRequest,
) -> Result<(), StorageError> {
    if request.source_revision.is_empty()
        || request.source_revision.len() > 256
        || invalid_text(&request.origin, 64)
        || invalid_text(&request.provider, 256)
        || invalid_text(&request.model, 256)
        || invalid_text(&request.remote_audio_url, 8_192)
        || invalid_optional(request.local_audio_url.as_deref(), 8_192)
        || invalid_optional(request.publisher_transcript_url.as_deref(), 8_192)
        || invalid_optional(request.publisher_mime_hint.as_deref(), 128)
        || (request.publisher_first && request.publisher_transcript_url.is_none())
        || (!request.publisher_first && !request.provider_fallback_enabled)
    {
        return Err(StorageError::TranscriptWorkflowConflict);
    }
    Ok(())
}

pub(super) fn validate_time(value: i64) -> Result<(), StorageError> {
    if value < 0 {
        Err(StorageError::TranscriptWorkflowConflict)
    } else {
        Ok(())
    }
}

pub(super) fn validate_detail(value: Option<&str>) -> Result<(), StorageError> {
    if value.is_some_and(|text| text.trim() != text || text.len() > 1_024) {
        Err(StorageError::TranscriptWorkflowConflict)
    } else {
        Ok(())
    }
}

pub(super) fn next_revision(value: StateRevision) -> Result<StateRevision, StorageError> {
    value
        .value
        .checked_add(1)
        .map(StateRevision::new)
        .ok_or(StorageError::TranscriptWorkflowConflict)
}

pub(super) fn i64_value(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::TranscriptWorkflowConflict)
}

pub(super) fn bytes16(value: Vec<u8>) -> Result<[u8; 16], StorageError> {
    value
        .try_into()
        .map_err(|_| StorageError::TranscriptWorkflowConflict)
}

pub(super) fn digest(value: Vec<u8>) -> Result<ContentDigest, StorageError> {
    value
        .try_into()
        .map(ContentDigest::from_bytes)
        .map_err(|_| StorageError::TranscriptWorkflowConflict)
}

pub(super) fn optional_id<T>(
    value: Option<Vec<u8>>,
    make: impl FnOnce([u8; 16]) -> T,
) -> Result<Option<T>, StorageError> {
    value.map(bytes16).transpose().map(|value| value.map(make))
}

pub(super) fn optional_digest(
    value: Option<Vec<u8>>,
) -> Result<Option<ContentDigest>, StorageError> {
    value.map(digest).transpose()
}

pub(super) fn unsigned(value: i64) -> Result<u64, StorageError> {
    u64::try_from(value).map_err(|_| StorageError::TranscriptWorkflowConflict)
}

pub(super) fn unsigned16(value: i64) -> Result<u16, StorageError> {
    u16::try_from(value).map_err(|_| StorageError::TranscriptWorkflowConflict)
}

pub(super) fn bool_value(value: i64) -> Result<bool, StorageError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(StorageError::TranscriptWorkflowConflict),
    }
}

fn invalid_text(value: &str, maximum: usize) -> bool {
    value.is_empty() || value.trim() != value || value.len() > maximum
}

fn invalid_optional(value: Option<&str>, maximum: usize) -> bool {
    value.is_some_and(|value| invalid_text(value, maximum))
}
