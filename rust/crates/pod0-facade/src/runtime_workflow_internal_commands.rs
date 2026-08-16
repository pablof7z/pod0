use pod0_application::{
    ChapterModelPlan, InternalCommandKind, MODEL_CHAPTER_REQUEST_DEADLINE_MILLISECONDS,
    MODEL_CHAPTER_WORKFLOW_MAX_ATTEMPTS, PUBLISHER_CHAPTER_MAX_ATTEMPTS,
    PUBLISHER_CHAPTER_REQUEST_DEADLINE_MILLISECONDS, RequestDisposition, RequestRejectionReason,
    publisher_chapter_source_version,
};
use pod0_domain::{CancellationId, CommandId, InternalCommandId};
use pod0_storage::{
    LibraryStore, ModelChapterDesiredPlan, ModelChapterEnsureInput, ModelChapterEnsureOutcome,
    PendingInternalCommand, PublisherChapterEnsureOutcome, PublisherChapterWorkflowState,
    ScheduledAgentCommandContext,
};
use sha2::{Digest as _, Sha256};

use crate::runtime_chapter_model_mapping::stored_model_request;
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn resume_workflow_internal_commands(&mut self) {
        let Some(store) = self.store.clone() else {
            return;
        };
        for _ in 0..100 {
            let Ok(commands) = store.pending_internal_commands(100) else {
                return;
            };
            let mut progressed = false;
            for command in commands {
                progressed |= match &command.request.kind {
                    InternalCommandKind::EnsurePublisherChapters => {
                        self.execute_publisher_internal(&store, command)
                    }
                    InternalCommandKind::EnsureTranscriptWorkflow { .. } => {
                        self.execute_playback_transcript_command(&store, command)
                    }
                    InternalCommandKind::EnsureModelChapters { .. } => {
                        self.execute_model_internal(&store, command)
                    }
                    InternalCommandKind::ReconcileScheduledRuns => {
                        self.execute_scheduled_internal(&store, command)
                    }
                    InternalCommandKind::ContinueWorkflowReconciliation { .. } => {
                        self.execute_reconcile_internal(&store, command)
                    }
                    _ => false,
                };
            }
            if !progressed {
                break;
            }
        }
    }

    fn execute_publisher_internal(
        &mut self,
        store: &LibraryStore,
        command: PendingInternalCommand,
    ) -> bool {
        if self.reload_listening().is_err() {
            return false;
        }
        let Some(episode_id) = command.request.episode_id else {
            return self.reject_internal(store, command, RequestRejectionReason::MissingSubject);
        };
        let source_url = self
            .listening
            .episodes
            .iter()
            .find(|episode| episode.episode_id == episode_id)
            .and_then(|episode| episode.feed_metadata.chapters_url.clone());
        let Some(source_url) = source_url else {
            return self.reject_internal(
                store,
                command,
                RequestRejectionReason::MissingPrerequisite,
            );
        };
        let Some(source_version) = publisher_chapter_source_version(&source_url) else {
            return self.reject_internal(store, command, RequestRejectionReason::Invalid);
        };
        let now = self.now().value;
        let result = store.ensure_publisher_chapter_workflow_from_internal_command(
            command.clone(),
            &source_url,
            &source_version,
            internal_cancellation_id(command.internal_command_id, b"publisher"),
            self.revision,
            now,
            now.saturating_add(PUBLISHER_CHAPTER_REQUEST_DEADLINE_MILLISECONDS),
            PUBLISHER_CHAPTER_MAX_ATTEMPTS,
        );
        match result {
            Ok(PublisherChapterEnsureOutcome::Requested { record, replaced }) => {
                let _ = replaced;
                self.advance_revision();
                let _ = record;
                true
            }
            Ok(PublisherChapterEnsureOutcome::Existing(record)) => {
                let _ = record;
                true
            }
            Err(_) => false,
        }
    }

    fn execute_model_internal(
        &mut self,
        store: &LibraryStore,
        command: PendingInternalCommand,
    ) -> bool {
        let InternalCommandKind::EnsureModelChapters { configured_model } =
            command.request.kind.clone()
        else {
            return false;
        };
        if self.reload_listening().is_err() {
            return false;
        }
        let Some(episode_id) = command.request.episode_id else {
            return self.reject_internal(store, command, RequestRejectionReason::MissingSubject);
        };
        let Some(episode) = self
            .listening
            .episodes
            .iter()
            .find(|episode| episode.episode_id == episode_id)
        else {
            return self.reject_internal(store, command, RequestRejectionReason::MissingSubject);
        };
        let desired_plan = if episode.feed_metadata.chapters_url.is_some()
            && store
                .publisher_chapter_workflow(episode_id)
                .ok()
                .flatten()
                .is_none_or(|record| record.state != PublisherChapterWorkflowState::Succeeded)
        {
            ModelChapterDesiredPlan::AwaitingPublisher
        } else {
            self.internal_model_plan(store, episode_id, &configured_model)
        };
        let now = self.now().value;
        let result = store.ensure_model_chapter_workflow_from_internal_command(
            command.clone(),
            ModelChapterEnsureInput {
                episode_id,
                configured_model,
                desired_plan,
                command_id: CommandId::from_bytes(command.internal_command_id.into_bytes()),
                cancellation_id: internal_cancellation_id(command.internal_command_id, b"model"),
                issued_revision: self.revision,
                now_ms: now,
                request_deadline_ms: now
                    .saturating_add(MODEL_CHAPTER_REQUEST_DEADLINE_MILLISECONDS),
                max_attempts: MODEL_CHAPTER_WORKFLOW_MAX_ATTEMPTS,
                force_retry_from_revision: None,
            },
        );
        match result {
            Ok(ModelChapterEnsureOutcome::Changed { record, replaced }) => {
                let _ = replaced;
                self.advance_revision();
                let _ = record;
                true
            }
            Ok(ModelChapterEnsureOutcome::Existing(record)) => {
                let _ = record;
                true
            }
            Err(_) => false,
        }
    }

    fn execute_scheduled_internal(
        &mut self,
        store: &LibraryStore,
        command: PendingInternalCommand,
    ) -> bool {
        let Some(scheduled) = self.scheduled_agent_store.clone() else {
            return self.reject_internal(
                store,
                command,
                RequestRejectionReason::StorageUnavailable,
            );
        };
        let now = self.now();
        let context = ScheduledAgentCommandContext {
            command_id: CommandId::from_bytes(command.internal_command_id.into_bytes()),
            command_fingerprint: command_fingerprint(&command),
            cancellation_id: internal_cancellation_id(command.internal_command_id, b"scheduled"),
            issued_revision: self.revision,
            observed_at: now,
        };
        if scheduled
            .reconcile_due_runs_from_internal_command(command, context)
            .is_err()
        {
            return false;
        }
        self.advance_revision();
        true
    }

    fn execute_reconcile_internal(
        &mut self,
        store: &LibraryStore,
        command: PendingInternalCommand,
    ) -> bool {
        let Ok(outcome) = store.continue_workflow_reconciliation_from_internal_command(command)
        else {
            return false;
        };
        self.revision = pod0_domain::StateRevision::new(
            self.revision
                .value
                .max(outcome.receipt.committed_revision.value),
        );
        true
    }

    fn reject_internal(
        &self,
        store: &LibraryStore,
        command: PendingInternalCommand,
        reason: RequestRejectionReason,
    ) -> bool {
        store
            .record_internal_command_disposition(
                command,
                self.revision,
                RequestDisposition::Rejected { reason },
                self.now(),
            )
            .is_ok()
    }

    fn internal_model_plan(
        &self,
        store: &LibraryStore,
        episode_id: pod0_domain::EpisodeId,
        configured_model: &str,
    ) -> ModelChapterDesiredPlan {
        match self.chapter_model_plan(episode_id, configured_model.to_owned()) {
            ChapterModelPlan::Ready { request } => stored_model_request(configured_model, request)
                .map(|request| ModelChapterDesiredPlan::Ready(Box::new(request)))
                .unwrap_or_else(|| blocked("invalid_request")),
            ChapterModelPlan::Current { artifact_id } => store
                .selected_chapter_artifact(episode_id)
                .ok()
                .flatten()
                .filter(|selection| selection.artifact.artifact_id == artifact_id)
                .map(|selection| ModelChapterDesiredPlan::Current {
                    artifact_id,
                    selection_revision: selection.selection_revision,
                })
                .unwrap_or_else(|| blocked("selection_changed")),
            ChapterModelPlan::TranscriptUnavailable => ModelChapterDesiredPlan::AwaitingTranscript,
            ChapterModelPlan::PreserveAgentComposed => store
                .selected_chapter_artifact(episode_id)
                .ok()
                .flatten()
                .map(|selection| ModelChapterDesiredPlan::PreserveAgentComposed {
                    artifact_id: selection.artifact.artifact_id,
                    selection_revision: selection.selection_revision,
                })
                .unwrap_or_else(|| blocked("invalid_request")),
            ChapterModelPlan::StaleTranscript => blocked("stale_transcript"),
            ChapterModelPlan::CoreUnavailable => blocked("storage_unavailable"),
            _ => blocked("invalid_request"),
        }
    }
}

fn blocked(code: &str) -> ModelChapterDesiredPlan {
    ModelChapterDesiredPlan::Blocked {
        failure_code: code.to_owned(),
        failure_detail: None,
    }
}

fn internal_cancellation_id(command: InternalCommandId, domain: &[u8]) -> CancellationId {
    let mut hash = Sha256::new();
    hash.update(b"pod0/workflow/internal-cancellation/v1");
    hash.update(domain);
    hash.update(command.into_bytes());
    CancellationId::from_bytes(hash.finalize()[..16].try_into().expect("digest prefix"))
}

fn command_fingerprint(command: &PendingInternalCommand) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"pod0/workflow/internal-scheduled-context/v1");
    hash.update(command.internal_command_id.into_bytes());
    hash.finalize().into()
}
