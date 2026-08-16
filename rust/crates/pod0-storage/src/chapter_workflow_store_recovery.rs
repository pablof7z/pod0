use pod0_domain::{CancellationId, CommandId, EpisodeId, StateRevision};

use crate::{LibraryStore, PublisherChapterEnsureOutcome, StorageError};

impl LibraryStore {
    #[allow(clippy::too_many_arguments)]
    pub fn recover_publisher_chapter_workflow(
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
    ) -> Result<PublisherChapterEnsureOutcome, StorageError> {
        crate::transition_commit::commit_publisher_chapter_admission(
            self.path(),
            episode_id,
            source_url,
            source_version,
            command_id,
            cancellation_id,
            issued_revision,
            now_ms,
            request_deadline_ms,
            max_attempts,
            false,
            true,
        )
    }
}
