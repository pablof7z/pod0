use pod0_domain::{ChapterArtifactId, EpisodeId, HostRequestId, StateRevision};
use rusqlite::{OptionalExtension, Transaction};
use sha2::{Digest as _, Sha256};

use crate::chapter_workflow_store_read::read_workflow;
use crate::{PublisherChapterWorkflowRecord, PublisherChapterWorkflowState, StorageError};

pub(crate) fn should_preserve(
    record: &PublisherChapterWorkflowRecord,
    source_version: &str,
    force_retry: bool,
) -> bool {
    if record.source_version != source_version {
        return false;
    }
    match record.state {
        PublisherChapterWorkflowState::Requested => true,
        PublisherChapterWorkflowState::RetryScheduled => !force_retry,
        PublisherChapterWorkflowState::Succeeded => record.selected_artifact_id.is_some(),
        PublisherChapterWorkflowState::Failed | PublisherChapterWorkflowState::Cancelled => {
            !force_retry
        }
        PublisherChapterWorkflowState::SourceAbsent => false,
    }
}

pub(crate) fn require_current_source(
    transaction: &Transaction<'_>,
    episode_id: EpisodeId,
    source_url: &str,
) -> Result<(), StorageError> {
    let stored: Option<String> = transaction
        .query_row(
            "SELECT chapters_url FROM pod0_episode_feed_metadata WHERE episode_id=?1",
            [episode_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read publisher chapter source", error))?;
    if stored.as_deref() == Some(source_url) {
        Ok(())
    } else {
        Err(StorageError::ChapterWorkflowConflict)
    }
}

pub(crate) fn selected_chapter(
    transaction: &Transaction<'_>,
    episode_id: EpisodeId,
) -> Result<Option<(ChapterArtifactId, StateRevision)>, StorageError> {
    let row: Option<(Vec<u8>, i64)> = transaction
        .query_row(
            "SELECT artifact_id,selection_revision FROM pod0_chapter_selections \
             WHERE episode_id=?1 ORDER BY selection_revision DESC LIMIT 1",
            [episode_id.into_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read publisher chapter selection", error))?;
    row.map(|(id, revision)| {
        Ok((
            ChapterArtifactId::from_bytes(
                id.try_into()
                    .map_err(|_| StorageError::ChapterWorkflowConflict)?,
            ),
            StateRevision::new(
                u64::try_from(revision).map_err(|_| StorageError::ChapterWorkflowConflict)?,
            ),
        ))
    })
    .transpose()
}

pub(crate) fn workflow_for_request(
    transaction: &Transaction<'_>,
    request_id: HostRequestId,
) -> Result<PublisherChapterWorkflowRecord, StorageError> {
    let episode: Option<Vec<u8>> = transaction
        .query_row(
            "SELECT episode_id FROM pod0_publisher_chapter_workflows WHERE request_id=?1 \
             AND state IN('requested','retry_scheduled')",
            [request_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("find publisher chapter request", error))?;
    let id: [u8; 16] = episode
        .ok_or(StorageError::ChapterWorkflowNotFound)?
        .try_into()
        .map_err(|_| StorageError::ChapterWorkflowConflict)?;
    read_workflow(transaction, EpisodeId::from_bytes(id))?
        .ok_or(StorageError::ChapterWorkflowNotFound)
}

pub(crate) fn request_id_for_generation(
    episode_id: EpisodeId,
    source_version: &str,
    generation: u64,
) -> HostRequestId {
    let mut hash = Sha256::new();
    hash.update(b"pod0.publisher-chapter-request.v1\0");
    hash.update(episode_id.into_bytes());
    hash.update(source_version.as_bytes());
    hash.update(generation.to_be_bytes());
    HostRequestId::from_bytes(hash.finalize()[..16].try_into().expect("digest length"))
}

pub(crate) fn i64_value(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::ChapterWorkflowConflict)
}
