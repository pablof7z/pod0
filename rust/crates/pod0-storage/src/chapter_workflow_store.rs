use pod0_domain::EpisodeId;

use crate::{
    LibraryStore, PublisherChapterWorkflowPage, PublisherChapterWorkflowRecord, StorageError,
};

impl LibraryStore {
    pub fn publisher_chapter_workflow(
        &self,
        episode_id: EpisodeId,
    ) -> Result<Option<PublisherChapterWorkflowRecord>, StorageError> {
        self.read(|connection| {
            crate::chapter_workflow_store_read::read_workflow(connection, episode_id)
        })
    }

    pub fn active_publisher_chapter_workflows(
        &self,
        max_items: u16,
    ) -> Result<Vec<PublisherChapterWorkflowRecord>, StorageError> {
        self.read(|connection| {
            crate::chapter_workflow_store_read::read_active_workflows(connection, max_items)
        })
    }

    pub fn publisher_chapter_workflow_page(
        &self,
        episode_id: Option<EpisodeId>,
        offset: u32,
        max_items: u16,
    ) -> Result<PublisherChapterWorkflowPage, StorageError> {
        self.read(|connection| {
            crate::chapter_workflow_store_read::read_workflow_page(
                connection, episode_id, offset, max_items,
            )
        })
    }

    pub fn mark_publisher_chapter_source_absent(
        &self,
        episode_id: EpisodeId,
        now_ms: i64,
    ) -> Result<Option<PublisherChapterWorkflowRecord>, StorageError> {
        self.write(|transaction| {
            let existing =
                crate::chapter_workflow_store_read::read_workflow(transaction, episode_id)?;
            if existing.is_some() {
                transaction
                    .execute(
                        "UPDATE pod0_publisher_chapter_workflows SET state='source_absent',\
                     workflow_revision=workflow_revision+1,request_id=NULL,deadline_at_ms=NULL,\
                     not_before_ms=NULL,failure_code=NULL,failure_detail=NULL,updated_at_ms=?1 \
                     WHERE episode_id=?2",
                        rusqlite::params![now_ms, episode_id.into_bytes().as_slice()],
                    )
                    .map_err(|error| {
                        StorageError::sqlite("mark publisher chapter source absent", error)
                    })?;
            }
            Ok(existing)
        })
    }
}
