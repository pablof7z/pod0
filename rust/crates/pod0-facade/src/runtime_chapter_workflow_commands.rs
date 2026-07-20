use pod0_application::{
    CommandEnvelope, CoreFailureCode, OperationStage, PUBLISHER_CHAPTER_MAX_ATTEMPTS,
    PUBLISHER_CHAPTER_REQUEST_DEADLINE_MILLISECONDS, publisher_chapter_source_version,
};
use pod0_domain::{CancellationId, CommandId, EpisodeId, StateRevision};
use pod0_storage::{PublisherChapterEnsureOutcome, PublisherChapterWorkflowState};

use crate::runtime_state::FacadeState;
use crate::runtime_storage_commands::storage_failure;

impl FacadeState {
    pub(super) fn ensure_publisher_chapters(
        &mut self,
        envelope: &CommandEnvelope,
        episode_id: EpisodeId,
    ) {
        let _ = self.start_publisher_chapter_workflow(
            episode_id,
            envelope.cancellation_id,
            envelope.command_id,
            false,
        );
    }

    pub(super) fn retry_publisher_chapters(
        &mut self,
        envelope: &CommandEnvelope,
        episode_id: EpisodeId,
        expected_revision: StateRevision,
    ) {
        let record = self
            .store
            .as_ref()
            .and_then(|store| store.publisher_chapter_workflow(episode_id).ok())
            .flatten();
        let Some(record) = record else {
            self.fail(envelope.command_id, CoreFailureCode::NotFound);
            return;
        };
        if record.workflow_revision != expected_revision {
            self.fail(envelope.command_id, CoreFailureCode::RevisionConflict);
            return;
        }
        if !matches!(
            record.state,
            PublisherChapterWorkflowState::Failed | PublisherChapterWorkflowState::Cancelled
        ) {
            self.fail(envelope.command_id, CoreFailureCode::InvalidCommand);
            return;
        }
        let _ = self.start_publisher_chapter_workflow(
            episode_id,
            envelope.cancellation_id,
            envelope.command_id,
            true,
        );
    }

    pub(super) fn cancel_publisher_chapters(
        &mut self,
        envelope: &CommandEnvelope,
        episode_id: EpisodeId,
        expected_revision: StateRevision,
    ) {
        let Some(store) = self.store.clone() else {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        };
        let existing = match store.publisher_chapter_workflow(episode_id) {
            Ok(Some(record)) => record,
            Ok(None) => {
                self.fail(envelope.command_id, CoreFailureCode::NotFound);
                return;
            }
            Err(error) => {
                self.fail(envelope.command_id, storage_failure(error));
                return;
            }
        };
        match store.cancel_publisher_chapter_workflow(
            episode_id,
            expected_revision,
            self.now().value,
        ) {
            Ok(_) => {
                self.withdraw_publisher_chapter_request(&existing);
                self.advance_revision();
                self.succeed(envelope.command_id, None);
            }
            Err(error) => self.fail(envelope.command_id, storage_failure(error)),
        }
    }

    pub(super) fn start_publisher_chapter_workflow(
        &mut self,
        episode_id: EpisodeId,
        cancellation_id: CancellationId,
        command_id: CommandId,
        force_retry: bool,
    ) -> bool {
        if let Err(error) = self.reload_listening() {
            self.fail(command_id, storage_failure(error));
            return false;
        }
        let Some(episode) = self
            .listening
            .episodes
            .iter()
            .find(|episode| episode.episode_id == episode_id)
            .cloned()
        else {
            self.fail(command_id, CoreFailureCode::NotFound);
            return true;
        };
        let Some(store) = self.store.clone() else {
            self.fail(command_id, CoreFailureCode::StorageUnavailable);
            return false;
        };
        let Some(source_url) = episode.feed_metadata.chapters_url else {
            match store.mark_publisher_chapter_source_absent(episode_id, self.now().value) {
                Ok(Some(record)) => self.withdraw_publisher_chapter_request(&record),
                Ok(None) => {}
                Err(error) => {
                    self.fail(command_id, storage_failure(error));
                    return false;
                }
            }
            self.succeed(command_id, None);
            return true;
        };
        let Some(source_version) = publisher_chapter_source_version(&source_url) else {
            self.fail(command_id, CoreFailureCode::InvalidCommand);
            return true;
        };
        let now = self.now().value;
        let Some(deadline_at) = now.checked_add(PUBLISHER_CHAPTER_REQUEST_DEADLINE_MILLISECONDS)
        else {
            self.fail(command_id, CoreFailureCode::InvalidCommand);
            return true;
        };
        let result = store.ensure_publisher_chapter_workflow(
            episode_id,
            &source_url,
            &source_version,
            command_id,
            cancellation_id,
            self.revision,
            now,
            deadline_at,
            PUBLISHER_CHAPTER_MAX_ATTEMPTS,
            force_retry,
        );
        match result {
            Ok(PublisherChapterEnsureOutcome::Requested { record, replaced }) => {
                if let Some(replaced) = replaced.as_deref() {
                    self.withdraw_publisher_chapter_request(replaced);
                }
                if self.queue_publisher_chapter_request(record) {
                    self.finish(command_id, OperationStage::Running, None, None);
                } else {
                    self.fail(command_id, CoreFailureCode::StorageUnavailable);
                }
                true
            }
            Ok(PublisherChapterEnsureOutcome::Existing(record)) => {
                if matches!(
                    record.state,
                    PublisherChapterWorkflowState::Requested
                        | PublisherChapterWorkflowState::RetryScheduled
                ) {
                    self.queue_publisher_chapter_request(record);
                }
                self.succeed(command_id, None);
                true
            }
            Err(error) => {
                self.fail(command_id, storage_failure(error));
                false
            }
        }
    }
}
