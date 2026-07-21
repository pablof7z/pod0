use pod0_domain::{
    ChapterArtifactId, ChapterArtifactSource, ChapterModelSubmissionFenceId, ContentDigest,
    EpisodeId, HostRequestId, StateRevision, TranscriptVersionId,
};
use rusqlite::{OptionalExtension, Transaction};
use sha2::{Digest as _, Sha256};

use super::model::{ModelChapterWorkflowMode, StoredModelChapterRequest};
use crate::StorageError;
use crate::chapter_store_read_artifact::read_chapter_artifact;

const MAX_COMPLETION_BYTES: u64 = 1_048_576;
const MAX_PROVIDER_BYTES: usize = 128;
const MAX_MODEL_BYTES: usize = 256;
const MAX_CONFIGURED_MODEL_BYTES: usize = 256;
const MAX_PROMPT_BYTES: usize = 1_048_576;

pub(super) fn validate_blocked_plan(
    failure_code: &str,
    failure_detail: Option<&str>,
) -> Result<(), StorageError> {
    if failure_code.is_empty()
        || failure_code.len() > 256
        || failure_detail.is_some_and(|value| value.len() > 16_384)
    {
        return Err(StorageError::ChapterWorkflowConflict);
    }
    Ok(())
}

pub(crate) fn validate_ensure_values(
    configured_model: &str,
    now_ms: i64,
    deadline_ms: i64,
    max_attempts: u16,
) -> Result<(), StorageError> {
    if now_ms < 0
        || deadline_ms < now_ms
        || max_attempts == 0
        || configured_model.len() > MAX_CONFIGURED_MODEL_BYTES
    {
        return Err(StorageError::ChapterWorkflowConflict);
    }
    Ok(())
}

pub(crate) fn validate_request(
    transaction: &Transaction<'_>,
    episode_id: EpisodeId,
    request: &StoredModelChapterRequest,
) -> Result<(), StorageError> {
    if request.source_version.is_empty()
        || request.format_version == 0
        || request.policy_version == 0
        || request.provider.is_empty()
        || request.provider.len() > MAX_PROVIDER_BYTES
        || request.model.is_empty()
        || request.model.len() > MAX_MODEL_BYTES
        || request.maximum_completion_bytes == 0
        || request.maximum_completion_bytes > MAX_COMPLETION_BYTES
        || request
            .system_prompt
            .len()
            .saturating_add(request.user_prompt.len())
            > MAX_PROMPT_BYTES
    {
        return Err(StorageError::ChapterWorkflowConflict);
    }
    validate_transcript(transaction, episode_id, request)?;
    validate_chapter_selection(transaction, episode_id, request)
}

fn validate_transcript(
    transaction: &Transaction<'_>,
    episode_id: EpisodeId,
    request: &StoredModelChapterRequest,
) -> Result<(), StorageError> {
    let selected: Option<(Vec<u8>, Vec<u8>)> = transaction
        .query_row(
            "SELECT selection.transcript_version_id,documents.content_digest \
             FROM pod0_transcript_selection selection JOIN pod0_transcript_documents documents \
             ON documents.transcript_version_id=selection.transcript_version_id \
             WHERE selection.episode_id=?1",
            [episode_id.into_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("validate model chapter transcript", error))?;
    let Some((version, digest)) = selected else {
        return Err(StorageError::ChapterWorkflowConflict);
    };
    let version = transcript_version(&version)?;
    let digest = content_digest(&digest)?;
    if version != request.requested_transcript_version_id
        || version != request.selected_transcript_version_id
        || digest != request.requested_transcript_digest
        || digest != request.selected_transcript_digest
    {
        return Err(StorageError::ChapterWorkflowConflict);
    }
    Ok(())
}

fn validate_chapter_selection(
    transaction: &Transaction<'_>,
    episode_id: EpisodeId,
    request: &StoredModelChapterRequest,
) -> Result<(), StorageError> {
    let selected: Option<(i64, Vec<u8>, Vec<u8>, i64)> = transaction
        .query_row(
            "SELECT selection.selection_revision,artifact.artifact_id,\
             artifact.integrity_digest,artifact.source_code FROM pod0_chapter_selections selection \
             JOIN pod0_chapter_artifacts artifact ON artifact.artifact_id=selection.artifact_id \
             WHERE selection.episode_id=?1 ORDER BY selection.selection_revision DESC LIMIT 1",
            [episode_id.into_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("validate model chapter selection", error))?;
    let revision = match selected.as_ref() {
        Some(row) => StateRevision::new(stored_u64(row.0)?),
        None => StateRevision::INITIAL,
    };
    if revision != request.expected_selection_revision {
        return Err(StorageError::ChapterWorkflowConflict);
    }
    match (request.mode, selected) {
        (ModelChapterWorkflowMode::Generate, _) if request.base_artifact_id.is_none() => {
            require_expected_source(request.expected_artifact_source, 2)
        }
        (ModelChapterWorkflowMode::Enrich, Some((_, id, digest, source))) => {
            let stored_id = artifact_id(&id)?;
            let stored_digest = content_digest(&digest)?;
            if !matches!(source, 1 | 3) {
                return Err(StorageError::ChapterWorkflowConflict);
            }
            let base_id = request
                .base_artifact_id
                .ok_or(StorageError::ChapterWorkflowConflict)?;
            let base_digest = request
                .base_integrity_digest
                .ok_or(StorageError::ChapterWorkflowConflict)?;
            let base = read_chapter_artifact(transaction, base_id)?
                .ok_or(StorageError::ChapterWorkflowConflict)?;
            if base.integrity_digest != base_digest
                || base.episode_id != episode_id
                || base.provenance.source != ChapterArtifactSource::Publisher
                || (source == 1 && (stored_id != base_id || stored_digest != base_digest))
            {
                return Err(StorageError::ChapterWorkflowConflict);
            }
            require_expected_source(request.expected_artifact_source, 3)
        }
        _ => Err(StorageError::ChapterWorkflowConflict),
    }
}

pub(crate) fn validate_preserved_selection(
    transaction: &Transaction<'_>,
    episode_id: EpisodeId,
    artifact_id: ChapterArtifactId,
    revision: StateRevision,
) -> Result<(), StorageError> {
    let matches: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pod0_chapter_selections selection \
             JOIN pod0_chapter_artifacts artifact ON artifact.artifact_id=selection.artifact_id \
             WHERE selection.episode_id=?1 AND selection.selection_revision=?2 \
             AND selection.artifact_id=?3 AND artifact.source_code=4)",
            rusqlite::params![
                episode_id.into_bytes().as_slice(),
                i64_value(revision.value)?,
                artifact_id.into_bytes().as_slice()
            ],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("validate preserved chapter selection", error))?;
    matches
        .then_some(())
        .ok_or(StorageError::ChapterWorkflowConflict)
}

pub(crate) fn validate_current_model_selection(
    transaction: &Transaction<'_>,
    episode_id: EpisodeId,
    artifact_id: ChapterArtifactId,
    revision: StateRevision,
) -> Result<(), StorageError> {
    let matches: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pod0_chapter_selections selection \
             JOIN pod0_chapter_artifacts artifact ON artifact.artifact_id=selection.artifact_id \
             WHERE selection.episode_id=?1 AND selection.selection_revision=?2 \
             AND selection.artifact_id=?3 AND artifact.source_code IN(2,3))",
            rusqlite::params![
                episode_id.into_bytes().as_slice(),
                i64_value(revision.value)?,
                artifact_id.into_bytes().as_slice()
            ],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("validate current model chapter selection", error))?;
    matches
        .then_some(())
        .ok_or(StorageError::ChapterWorkflowConflict)
}

fn require_expected_source(
    source: ChapterArtifactSource,
    expected: i64,
) -> Result<(), StorageError> {
    (artifact_source_code(source)? == expected)
        .then_some(())
        .ok_or(StorageError::ChapterWorkflowConflict)
}

pub(crate) fn request_id(
    episode_id: EpisodeId,
    fingerprint: ContentDigest,
    generation: u64,
) -> HostRequestId {
    let mut hash = Sha256::new();
    hash.update(b"pod0.model-chapter-request.v1\0");
    hash.update(episode_id.into_bytes());
    hash.update(fingerprint.into_bytes());
    hash.update(generation.to_be_bytes());
    HostRequestId::from_bytes(hash.finalize()[..16].try_into().expect("digest length"))
}

pub(crate) fn submission_fence_id(
    episode_id: EpisodeId,
    request_id: HostRequestId,
    cancellation_id: pod0_domain::CancellationId,
    issued_revision: StateRevision,
) -> ChapterModelSubmissionFenceId {
    let mut hash = Sha256::new();
    hash.update(b"pod0.model-chapter-submission-fence.v1\0");
    hash.update(episode_id.into_bytes());
    hash.update(request_id.into_bytes());
    hash.update(cancellation_id.into_bytes());
    hash.update(issued_revision.value.to_be_bytes());
    ChapterModelSubmissionFenceId::from_bytes(
        hash.finalize()[..16].try_into().expect("digest length"),
    )
}

pub(crate) fn artifact_source_code(source: ChapterArtifactSource) -> Result<i64, StorageError> {
    match source {
        ChapterArtifactSource::Publisher => Ok(1),
        ChapterArtifactSource::Generated => Ok(2),
        ChapterArtifactSource::PublisherEnriched => Ok(3),
        ChapterArtifactSource::AgentComposed => Ok(4),
        ChapterArtifactSource::Unsupported { .. } => Err(StorageError::ChapterWorkflowConflict),
    }
}

pub(crate) fn i64_value(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::ChapterWorkflowConflict)
}

fn stored_u64(value: i64) -> Result<u64, StorageError> {
    u64::try_from(value).map_err(|_| StorageError::ChapterWorkflowConflict)
}

fn artifact_id(value: &[u8]) -> Result<ChapterArtifactId, StorageError> {
    value
        .try_into()
        .map(ChapterArtifactId::from_bytes)
        .map_err(|_| StorageError::ChapterWorkflowConflict)
}

fn transcript_version(value: &[u8]) -> Result<TranscriptVersionId, StorageError> {
    value
        .try_into()
        .map(TranscriptVersionId::from_bytes)
        .map_err(|_| StorageError::ChapterWorkflowConflict)
}

fn content_digest(value: &[u8]) -> Result<ContentDigest, StorageError> {
    value
        .try_into()
        .map(ContentDigest::from_bytes)
        .map_err(|_| StorageError::ChapterWorkflowConflict)
}
