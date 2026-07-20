use pod0_domain::{CancellationId, CommandId, EpisodeId, StateRevision};
use rusqlite::params;

use crate::chapter_workflow_store_adopt::adopt_current_publisher_artifact;
use crate::chapter_workflow_store_read::read_workflow;
use crate::chapter_workflow_store_support::{
    i64_value, request_id_for_generation, require_current_source, selected_chapter,
    should_preserve, workflow_for_request,
};
use crate::{
    LibraryStore, PublisherChapterEnsureOutcome, PublisherChapterWorkflowFailureInput,
    PublisherChapterWorkflowRecord, PublisherChapterWorkflowState, PublisherChapterWorkflowUpdate,
    StorageError,
};

impl LibraryStore {
    #[allow(clippy::too_many_arguments)]
    pub fn ensure_publisher_chapter_workflow(
        &self,
        episode_id: EpisodeId,
        source_url: &str,
        source_version: &str,
        command_id: CommandId,
        cancellation_id: CancellationId,
        issued_revision: StateRevision,
        now_ms: i64,
        request_deadline_ms: i64,
        max_attempts: u16,
        force_retry: bool,
    ) -> Result<PublisherChapterEnsureOutcome, StorageError> {
        self.write(|transaction| {
            require_current_source(transaction, episode_id, source_url)?;
            let existing = read_workflow(transaction, episode_id)?;
            let selected = selected_chapter(transaction, episode_id)?;
            if let Some(record) = existing.as_ref()
                && should_preserve(record, source_version, force_retry)
            {
                return Ok(PublisherChapterEnsureOutcome::Existing(record.clone()));
            }
            if existing.is_none()
                && let Some(adopted) = adopt_current_publisher_artifact(
                    transaction,
                    episode_id,
                    source_url,
                    source_version,
                    command_id,
                    cancellation_id,
                    issued_revision,
                    now_ms,
                    max_attempts,
                )?
            {
                return Ok(PublisherChapterEnsureOutcome::Existing(adopted));
            }
            let generation = existing.as_ref().map_or(Ok(1), |record| {
                record
                    .generation
                    .checked_add(1)
                    .ok_or(StorageError::ChapterWorkflowConflict)
            })?;
            let same_source = existing
                .as_ref()
                .is_some_and(|record| record.source_version == source_version);
            let attempt = if same_source && !force_retry {
                existing.as_ref().map_or(Ok(1), |record| {
                    record
                        .attempt
                        .checked_add(1)
                        .ok_or(StorageError::ChapterWorkflowConflict)
                })?
            } else {
                1
            };
            let workflow_revision = existing.as_ref().map_or(Ok(1), |record| {
                record
                    .workflow_revision
                    .value
                    .checked_add(1)
                    .ok_or(StorageError::ChapterWorkflowConflict)
            })?;
            let created_at_ms = existing
                .as_ref()
                .map_or(now_ms, |record| record.created_at_ms);
            let expected_selection_revision =
                selected.map_or(StateRevision::INITIAL, |item| item.1);
            let request_id = request_id_for_generation(episode_id, source_version, generation);
            transaction
                .execute(
                    "INSERT INTO pod0_publisher_chapter_workflows(episode_id,source_url,\
                     source_version,state,generation,workflow_revision,attempt,max_attempts,\
                     command_id,cancellation_id,request_id,issued_revision,\
                     expected_selection_revision,deadline_at_ms,not_before_ms,\
                     selected_artifact_id,failure_code,failure_detail,created_at_ms,updated_at_ms) \
                     VALUES(?1,?2,?3,'requested',?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,\
                     NULL,NULL,NULL,NULL,?14,?15) ON CONFLICT(episode_id) DO UPDATE SET \
                     source_url=excluded.source_url,source_version=excluded.source_version,\
                     state='requested',generation=excluded.generation,\
                     workflow_revision=excluded.workflow_revision,attempt=excluded.attempt,\
                     max_attempts=excluded.max_attempts,command_id=excluded.command_id,\
                     cancellation_id=excluded.cancellation_id,request_id=excluded.request_id,\
                     issued_revision=excluded.issued_revision,\
                     expected_selection_revision=excluded.expected_selection_revision,\
                     deadline_at_ms=excluded.deadline_at_ms,not_before_ms=NULL,\
                     selected_artifact_id=NULL,failure_code=NULL,failure_detail=NULL,\
                     updated_at_ms=excluded.updated_at_ms",
                    params![
                        episode_id.into_bytes().as_slice(),
                        source_url,
                        source_version,
                        i64_value(generation)?,
                        i64_value(workflow_revision)?,
                        i64::from(attempt),
                        i64::from(max_attempts),
                        command_id.into_bytes().as_slice(),
                        cancellation_id.into_bytes().as_slice(),
                        request_id.into_bytes().as_slice(),
                        i64_value(issued_revision.value)?,
                        i64_value(expected_selection_revision.value)?,
                        request_deadline_ms,
                        created_at_ms,
                        now_ms,
                    ],
                )
                .map_err(|error| {
                    StorageError::sqlite("ensure publisher chapter workflow", error)
                })?;
            let record = read_workflow(transaction, episode_id)?
                .ok_or(StorageError::ChapterWorkflowNotFound)?;
            Ok(PublisherChapterEnsureOutcome::Requested {
                record,
                replaced: existing
                    .filter(|record| {
                        matches!(
                            record.state,
                            PublisherChapterWorkflowState::Requested
                                | PublisherChapterWorkflowState::RetryScheduled
                        )
                    })
                    .map(Box::new),
            })
        })
    }

    pub fn fail_publisher_chapter_workflow(
        &self,
        input: PublisherChapterWorkflowFailureInput,
    ) -> Result<PublisherChapterWorkflowUpdate, StorageError> {
        self.write(|transaction| {
            let record = workflow_for_request(transaction, input.request_id)?;
            let retryable = input.retry_at_ms.is_some() && record.attempt < record.max_attempts;
            let state = if retryable {
                PublisherChapterWorkflowState::RetryScheduled
            } else {
                PublisherChapterWorkflowState::Failed
            };
            let (generation, attempt, next_request_id, deadline_at_ms, not_before_ms) = if retryable
            {
                let generation = record
                    .generation
                    .checked_add(1)
                    .ok_or(StorageError::ChapterWorkflowConflict)?;
                (
                    generation,
                    record
                        .attempt
                        .checked_add(1)
                        .ok_or(StorageError::ChapterWorkflowConflict)?,
                    Some(request_id_for_generation(
                        record.episode_id,
                        &record.source_version,
                        generation,
                    )),
                    input.retry_deadline_at_ms,
                    input.retry_at_ms,
                )
            } else {
                (
                    record.generation,
                    record.attempt,
                    record.request_id,
                    None,
                    None,
                )
            };
            transaction
                .execute(
                    "UPDATE pod0_publisher_chapter_workflows SET state=?1,generation=?2,\
                     workflow_revision=workflow_revision+1,attempt=?3,request_id=?4,\
                     issued_revision=?5,deadline_at_ms=?6,not_before_ms=?7,failure_code=?8,\
                     failure_detail=?9,updated_at_ms=?10 WHERE episode_id=?11 AND request_id=?12 \
                     AND state IN('requested','retry_scheduled')",
                    params![
                        state.wire(),
                        i64_value(generation)?,
                        i64::from(attempt),
                        next_request_id.map(|id| id.into_bytes().to_vec()),
                        i64_value(input.retry_issued_revision.value)?,
                        deadline_at_ms,
                        not_before_ms,
                        input.failure_code,
                        input.failure_detail,
                        input.observed_at_ms,
                        record.episode_id.into_bytes().as_slice(),
                        input.request_id.into_bytes().as_slice(),
                    ],
                )
                .map_err(|error| StorageError::sqlite("fail publisher chapter workflow", error))?;
            if transaction.changes() != 1 {
                return Err(StorageError::ChapterWorkflowConflict);
            }
            let updated = read_workflow(transaction, record.episode_id)?
                .ok_or(StorageError::ChapterWorkflowNotFound)?;
            Ok(if retryable {
                PublisherChapterWorkflowUpdate::RetryScheduled(updated)
            } else {
                PublisherChapterWorkflowUpdate::Failed(updated)
            })
        })
    }

    pub fn cancel_publisher_chapter_workflow(
        &self,
        episode_id: EpisodeId,
        expected_revision: StateRevision,
        now_ms: i64,
    ) -> Result<PublisherChapterWorkflowRecord, StorageError> {
        self.write(|transaction| {
            transaction
                .execute(
                    "UPDATE pod0_publisher_chapter_workflows SET state='cancelled',\
                 workflow_revision=workflow_revision+1,deadline_at_ms=NULL,not_before_ms=NULL,\
                 failure_code=NULL,failure_detail=NULL,updated_at_ms=?1 WHERE episode_id=?2 \
                 AND workflow_revision=?3 AND state IN('requested','retry_scheduled')",
                    params![
                        now_ms,
                        episode_id.into_bytes().as_slice(),
                        i64_value(expected_revision.value)?
                    ],
                )
                .map_err(|error| {
                    StorageError::sqlite("cancel publisher chapter workflow", error)
                })?;
            if transaction.changes() != 1 {
                return Err(StorageError::ChapterWorkflowConflict);
            }
            read_workflow(transaction, episode_id)?.ok_or(StorageError::ChapterWorkflowNotFound)
        })
    }
}
