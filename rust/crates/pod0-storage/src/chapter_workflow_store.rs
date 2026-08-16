use pod0_domain::EpisodeId;
use rusqlite::OptionalExtension;

use crate::{
    LibraryStore, PublisherChapterWorkflowPage, PublisherChapterWorkflowRecord, StorageError,
};

impl LibraryStore {
    #[allow(clippy::too_many_arguments)]
    pub fn ensure_publisher_chapter_workflow_from_internal_command(
        &self,
        command: crate::PendingInternalCommand,
        source_url: &str,
        source_version: &str,
        cancellation_id: pod0_domain::CancellationId,
        issued_revision: pod0_domain::StateRevision,
        now_ms: i64,
        request_deadline_ms: i64,
        max_attempts: u16,
    ) -> Result<crate::PublisherChapterEnsureOutcome, StorageError> {
        crate::transition_commit::commit_publisher_chapter_internal_admission(
            self.path(),
            command,
            source_url,
            source_version,
            cancellation_id,
            issued_revision,
            now_ms,
            request_deadline_ms,
            max_attempts,
        )
    }

    pub fn cancel_publisher_chapter_command(
        &self,
        command_id: pod0_domain::CommandId,
        fingerprint: pod0_domain::ContentDigest,
        episode_id: EpisodeId,
        expected_revision: pod0_domain::StateRevision,
        now_ms: i64,
    ) -> Result<PublisherChapterWorkflowRecord, StorageError> {
        crate::transition_commit::commit_publisher_chapter_cancellation(
            self.path(),
            command_id,
            fingerprint,
            episode_id,
            expected_revision,
            now_ms,
        )
    }

    pub fn publisher_chapter_workflow(
        &self,
        episode_id: EpisodeId,
    ) -> Result<Option<PublisherChapterWorkflowRecord>, StorageError> {
        self.read(|connection| {
            crate::chapter_workflow_store_read::read_workflow(connection, episode_id)
        })
    }

    pub fn publisher_chapter_workflow_for_effect_intent(
        &self,
        intent_id: pod0_domain::EffectIntentId,
    ) -> Result<Option<PublisherChapterWorkflowRecord>, StorageError> {
        self.read(|connection| {
            let episode: Option<Vec<u8>> = connection
                .query_row(
                    "SELECT episode_id FROM pod0_effect_intents WHERE intent_id=?1
                     AND effect_kind_code=4 AND subject_code=2",
                    [intent_id.into_bytes().as_slice()],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|error| {
                    StorageError::sqlite("read publisher chapter effect workflow", error)
                })?;
            episode
                .map(|bytes| {
                    let episode_id = EpisodeId::from_bytes(
                        bytes
                            .try_into()
                            .map_err(|_| StorageError::InvalidActivity)?,
                    );
                    crate::chapter_workflow_store_read::read_workflow(connection, episode_id)
                })
                .transpose()
                .map(Option::flatten)
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
        command_id: pod0_domain::CommandId,
        now_ms: i64,
        recovery: bool,
    ) -> Result<Option<PublisherChapterWorkflowRecord>, StorageError> {
        crate::transition_commit::commit_publisher_chapter_source_absent(
            self.path(),
            episode_id,
            command_id,
            now_ms,
            recovery,
        )
    }
}
