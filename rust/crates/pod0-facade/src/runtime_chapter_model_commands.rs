use pod0_application::{
    ChapterModelPlan, CommandEnvelope, CoreFailureCode,
    MODEL_CHAPTER_REQUEST_DEADLINE_MILLISECONDS, MODEL_CHAPTER_WORKFLOW_MAX_ATTEMPTS,
    OperationStage,
};
use pod0_domain::{EpisodeId, StateRevision};
use pod0_storage::{
    LibraryStore, ModelChapterDesiredPlan, ModelChapterEnsureInput, ModelChapterEnsureOutcome,
    ModelChapterWorkflowState,
};

use crate::runtime_chapter_model_mapping::stored_model_request;
use crate::runtime_state::FacadeState;
use crate::runtime_storage_commands::storage_failure;

impl FacadeState {
    pub(super) fn ensure_model_chapters(
        &mut self,
        envelope: &CommandEnvelope,
        episode_id: EpisodeId,
        configured_model: String,
    ) {
        let _ = self.start_model_chapter_workflow(envelope, episode_id, configured_model, None);
    }

    pub(super) fn retry_model_chapters(
        &mut self,
        envelope: &CommandEnvelope,
        episode_id: EpisodeId,
        configured_model: String,
        expected_revision: StateRevision,
    ) {
        if self.authoritative_model_chapter_store(envelope).is_none() {
            return;
        }
        let record = self
            .store
            .as_ref()
            .and_then(|store| store.model_chapter_workflow(episode_id).ok())
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
            ModelChapterWorkflowState::Ambiguous
                | ModelChapterWorkflowState::Blocked
                | ModelChapterWorkflowState::Failed
                | ModelChapterWorkflowState::Cancelled
        ) || record.attempt >= record.max_attempts
        {
            self.fail(envelope.command_id, CoreFailureCode::InvalidCommand);
            return;
        }
        let _ = self.start_model_chapter_workflow(
            envelope,
            episode_id,
            configured_model,
            Some(expected_revision),
        );
    }

    pub(super) fn cancel_model_chapters(
        &mut self,
        envelope: &CommandEnvelope,
        episode_id: EpisodeId,
        expected_revision: StateRevision,
    ) {
        let Some(store) = self.authoritative_model_chapter_store(envelope) else {
            return;
        };
        let existing = match store.model_chapter_workflow(episode_id) {
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
        match store.cancel_model_chapter_workflow(episode_id, expected_revision, self.now().value) {
            Ok(_) => {
                self.withdraw_model_chapter_request(&existing);
                self.advance_revision();
                self.succeed(envelope.command_id, None);
            }
            Err(error) => self.fail(envelope.command_id, storage_failure(error)),
        }
    }

    fn start_model_chapter_workflow(
        &mut self,
        envelope: &CommandEnvelope,
        episode_id: EpisodeId,
        configured_model: String,
        force_retry_from_revision: Option<StateRevision>,
    ) -> bool {
        let Some(store) = self.authoritative_model_chapter_store(envelope) else {
            return false;
        };
        if let Err(error) = self.reload_listening() {
            self.fail(envelope.command_id, storage_failure(error));
            return false;
        }
        let Some(episode) = self
            .listening
            .episodes
            .iter()
            .find(|episode| episode.episode_id == episode_id)
        else {
            self.fail(envelope.command_id, CoreFailureCode::NotFound);
            return true;
        };
        let desired_plan = if episode.feed_metadata.chapters_url.is_some()
            && store
                .publisher_chapter_workflow(episode_id)
                .ok()
                .flatten()
                .is_none_or(|record| {
                    record.state != pod0_storage::PublisherChapterWorkflowState::Succeeded
                }) {
            ModelChapterDesiredPlan::AwaitingPublisher
        } else {
            self.model_desired_plan(episode_id, &configured_model)
        };
        let now = self.now().value;
        let Some(deadline) = now.checked_add(MODEL_CHAPTER_REQUEST_DEADLINE_MILLISECONDS) else {
            self.fail(envelope.command_id, CoreFailureCode::InvalidCommand);
            return true;
        };
        let result = store.ensure_model_chapter_workflow(ModelChapterEnsureInput {
            episode_id,
            configured_model,
            desired_plan,
            command_id: envelope.command_id,
            cancellation_id: envelope.cancellation_id,
            issued_revision: self.revision,
            now_ms: now,
            request_deadline_ms: deadline,
            max_attempts: MODEL_CHAPTER_WORKFLOW_MAX_ATTEMPTS,
            force_retry_from_revision,
        });
        match result {
            Ok(ModelChapterEnsureOutcome::Changed { record, replaced }) => {
                if let Some(replaced) = replaced.as_deref() {
                    self.withdraw_model_chapter_request(replaced);
                }
                self.advance_revision();
                if matches!(
                    record.state,
                    ModelChapterWorkflowState::Requested
                        | ModelChapterWorkflowState::RetryScheduled
                ) {
                    self.queue_model_chapter_request(&record);
                    self.finish(envelope.command_id, OperationStage::Running, None, None);
                } else {
                    self.succeed(envelope.command_id, None);
                }
                true
            }
            Ok(ModelChapterEnsureOutcome::Existing(record)) => {
                if matches!(
                    record.state,
                    ModelChapterWorkflowState::Requested
                        | ModelChapterWorkflowState::RetryScheduled
                ) {
                    self.queue_model_chapter_request(&record);
                }
                self.succeed(envelope.command_id, None);
                true
            }
            Err(error) => {
                self.fail(envelope.command_id, storage_failure(error));
                false
            }
        }
    }

    fn authoritative_model_chapter_store(
        &mut self,
        envelope: &CommandEnvelope,
    ) -> Option<LibraryStore> {
        let Some(store) = self.store.clone() else {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return None;
        };
        match store.model_chapter_workflow_authority() {
            Ok(state) if state.is_authoritative() => Some(store),
            Ok(_) => {
                self.fail(envelope.command_id, CoreFailureCode::HostUnavailable);
                None
            }
            Err(error) => {
                self.fail(envelope.command_id, storage_failure(error));
                None
            }
        }
    }

    fn model_desired_plan(
        &self,
        episode_id: EpisodeId,
        configured_model: &str,
    ) -> ModelChapterDesiredPlan {
        match self.chapter_model_plan(episode_id, configured_model.to_owned()) {
            ChapterModelPlan::Ready { request } => stored_model_request(configured_model, request)
                .map(|request| ModelChapterDesiredPlan::Ready(Box::new(request)))
                .unwrap_or_else(|| blocked("invalid_request")),
            ChapterModelPlan::Current { artifact_id } => self
                .store
                .as_ref()
                .and_then(|store| store.selected_chapter_artifact(episode_id).ok())
                .flatten()
                .filter(|selection| selection.artifact.artifact_id == artifact_id)
                .map(|selection| ModelChapterDesiredPlan::Current {
                    artifact_id,
                    selection_revision: selection.selection_revision,
                })
                .unwrap_or_else(|| blocked("selection_changed")),
            ChapterModelPlan::TranscriptUnavailable => ModelChapterDesiredPlan::AwaitingTranscript,
            ChapterModelPlan::PreserveAgentComposed => self
                .store
                .as_ref()
                .and_then(|store| store.selected_chapter_artifact(episode_id).ok())
                .flatten()
                .map(|selection| ModelChapterDesiredPlan::PreserveAgentComposed {
                    artifact_id: selection.artifact.artifact_id,
                    selection_revision: selection.selection_revision,
                })
                .unwrap_or_else(|| blocked("invalid_request")),
            ChapterModelPlan::StaleTranscript => blocked("stale_transcript"),
            ChapterModelPlan::CoreUnavailable => blocked("storage_unavailable"),
            ChapterModelPlan::EpisodeUnavailable
            | ChapterModelPlan::InvalidConfiguration
            | ChapterModelPlan::UnsupportedArtifact
            | ChapterModelPlan::InvalidInput
            | ChapterModelPlan::EmptyTranscript
            | ChapterModelPlan::InputTooLarge => blocked("invalid_request"),
        }
    }
}

fn blocked(code: &str) -> ModelChapterDesiredPlan {
    ModelChapterDesiredPlan::Blocked {
        failure_code: code.to_owned(),
        failure_detail: None,
    }
}
