use pod0_domain::{ChapterArtifact, ChapterArtifactInput, CommandId, HostRequestId};
use rusqlite::params;

use crate::chapter_workflow_store_read::read_workflow;
use crate::chapter_workflow_store_support::{
    require_current_source, selected_chapter, workflow_for_request,
};
use crate::library_store_chapters::commit_and_select_chapter_in_transaction;
use crate::{LibraryStore, PublisherChapterWorkflowRecord, StorageError};

impl LibraryStore {
    pub fn complete_publisher_chapter_workflow(
        &self,
        request_id: HostRequestId,
        input: ChapterArtifactInput,
        completed_at_ms: i64,
    ) -> Result<PublisherChapterWorkflowRecord, StorageError> {
        let artifact =
            ChapterArtifact::seal(input).map_err(|_| StorageError::InvalidChapterArtifact)?;
        self.write(|transaction| {
            let record = workflow_for_request(transaction, request_id)?;
            require_current_source(transaction, record.episode_id, &record.source_url)?;
            if artifact.episode_id != record.episode_id {
                return Err(StorageError::ChapterWorkflowConflict);
            }
            let selected = selected_chapter(transaction, record.episode_id)?;
            if selected.map(|item| item.0) != Some(artifact.artifact_id) {
                if selected.map_or(0, |item| item.1.value)
                    != record.expected_selection_revision.value
                {
                    return Err(StorageError::ChapterRevisionConflict);
                }
                commit_and_select_chapter_in_transaction(
                    transaction,
                    CommandId::from_bytes(request_id.into_bytes()),
                    record.expected_selection_revision,
                    &artifact,
                    completed_at_ms,
                    || Ok(()),
                )?;
            }
            transaction
                .execute(
                    "UPDATE pod0_publisher_chapter_workflows SET state='succeeded',\
                     workflow_revision=workflow_revision+1,deadline_at_ms=NULL,not_before_ms=NULL,\
                     selected_artifact_id=?1,failure_code=NULL,failure_detail=NULL,updated_at_ms=?2 \
                     WHERE episode_id=?3 AND request_id=?4 \
                     AND state IN('requested','retry_scheduled')",
                    params![
                        artifact.artifact_id.into_bytes().as_slice(),
                        completed_at_ms,
                        record.episode_id.into_bytes().as_slice(),
                        request_id.into_bytes().as_slice()
                    ],
                )
                .map_err(|error| {
                    StorageError::sqlite("complete publisher chapter workflow", error)
                })?;
            if transaction.changes() != 1 {
                return Err(StorageError::ChapterWorkflowConflict);
            }
            read_workflow(transaction, record.episode_id)?
                .ok_or(StorageError::ChapterWorkflowNotFound)
        })
    }
}
