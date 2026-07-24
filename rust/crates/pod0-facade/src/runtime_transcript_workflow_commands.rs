use pod0_application::{
    CommandEnvelope, CoreFailureCode, OperationStage,
    TRANSCRIPT_HOST_REQUEST_DEADLINE_MILLISECONDS, TRANSCRIPT_WORKFLOW_MAX_ATTEMPTS,
    TranscriptGenerationDecision, TranscriptWorkflowConfiguration, TranscriptWorkflowOrigin,
    transcript_attempt_id, transcript_submission_fence_id,
};
use pod0_domain::{EpisodeId, StateRevision};
use pod0_storage::{
    LibraryStore, PreparedTranscriptAttempt, StoredTranscriptWorkflowStage,
    TranscriptWorkflowEnsureInput, TranscriptWorkflowEnsureOutcome,
};

use crate::runtime_state::{FacadeState, failure};
use crate::runtime_storage_commands::storage_failure;
use crate::runtime_transcript_workflow_mapping::{request_id, stored_request};

impl FacadeState {
    pub(super) fn ensure_transcript_workflow(
        &mut self,
        envelope: &CommandEnvelope,
        episode_id: EpisodeId,
        origin: TranscriptWorkflowOrigin,
        configuration: TranscriptWorkflowConfiguration,
    ) {
        if !self.transcript_origin_is_allowed(episode_id, origin) {
            self.succeed(envelope.command_id, None);
            return;
        }
        self.start_transcript_workflow(envelope, episode_id, origin, configuration, None);
    }

    pub(super) fn retry_transcript_workflow(
        &mut self,
        envelope: &CommandEnvelope,
        episode_id: EpisodeId,
        expected_revision: StateRevision,
        configuration: TranscriptWorkflowConfiguration,
    ) {
        self.start_transcript_workflow(
            envelope,
            episode_id,
            TranscriptWorkflowOrigin::User,
            configuration,
            Some(expected_revision),
        );
    }

    pub(super) fn cancel_transcript_workflow(
        &mut self,
        envelope: &CommandEnvelope,
        episode_id: EpisodeId,
        expected_revision: StateRevision,
    ) {
        let Some(store) = self.authoritative_transcript_workflow_store(envelope) else {
            return;
        };
        let existing = match store.transcript_workflow(episode_id) {
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
        match store.cancel_transcript_workflow(episode_id, expected_revision, self.now().value) {
            Ok(_) => {
                self.withdraw_transcript_request(&existing);
                self.advance_revision();
                self.succeed(envelope.command_id, None);
            }
            Err(error) => self.fail(envelope.command_id, storage_failure(error)),
        }
    }

    fn start_transcript_workflow(
        &mut self,
        envelope: &CommandEnvelope,
        episode_id: EpisodeId,
        origin: TranscriptWorkflowOrigin,
        configuration: TranscriptWorkflowConfiguration,
        force_retry_from_revision: Option<StateRevision>,
    ) {
        let Some(store) = self.authoritative_transcript_workflow_store(envelope) else {
            return;
        };
        if let Err(error) = self.reload_listening() {
            self.fail(envelope.command_id, storage_failure(error));
            return;
        }
        let Some(runtime_plan) = self.transcript_workflow_plan(episode_id, origin, configuration)
        else {
            self.fail(envelope.command_id, CoreFailureCode::InvalidCommand);
            return;
        };
        if runtime_plan.is_current() {
            self.succeed(envelope.command_id, None);
            return;
        }
        let request = match runtime_plan.plan.generation {
            TranscriptGenerationDecision::Ensure => runtime_plan.plan.request,
            TranscriptGenerationDecision::Current => {
                self.start_current_transcript_evidence(envelope, episode_id);
                return;
            }
            TranscriptGenerationDecision::AwaitingCredential { .. }
            | TranscriptGenerationDecision::AwaitingLocalAudio => {
                self.finish(
                    envelope.command_id,
                    OperationStage::Blocked,
                    Some(failure(CoreFailureCode::HostUnavailable)),
                    None,
                );
                return;
            }
            TranscriptGenerationDecision::Blocked { .. } => {
                self.fail(envelope.command_id, CoreFailureCode::InvalidCommand);
                return;
            }
            TranscriptGenerationDecision::NotRequested => {
                self.succeed(envelope.command_id, None);
                return;
            }
        };
        let Some(request) = request else {
            self.fail(envelope.command_id, CoreFailureCode::InvalidCommand);
            return;
        };
        let existing = match store.transcript_workflow(episode_id) {
            Ok(value) => value,
            Err(error) => {
                self.fail(envelope.command_id, storage_failure(error));
                return;
            }
        };
        if force_retry_from_revision.is_some()
            && existing.as_ref().map(|value| value.workflow_revision) != force_retry_from_revision
        {
            self.fail(envelope.command_id, CoreFailureCode::RevisionConflict);
            return;
        }
        let now = self.now().value;
        let deadline = now.saturating_add(TRANSCRIPT_HOST_REQUEST_DEADLINE_MILLISECONDS);
        let attempt_number = existing
            .as_ref()
            .map_or(1, |value| value.attempt.saturating_add(1));
        let publisher = request.publisher_first;
        let prepared_attempt = (!publisher)
            .then(|| transcript_attempt_id(request.workflow_id, attempt_number))
            .flatten()
            .map(|attempt_id| PreparedTranscriptAttempt {
                attempt: attempt_number,
                attempt_id,
                submission_fence_id: transcript_submission_fence_id(attempt_id),
            });
        let host_request_id = request_id(request.workflow_id, attempt_number, publisher);
        let outcome = store.ensure_transcript_workflow(TranscriptWorkflowEnsureInput {
            episode_id,
            request: stored_request(request),
            stage: if publisher {
                StoredTranscriptWorkflowStage::PublisherRequested
            } else {
                StoredTranscriptWorkflowStage::Requested
            },
            prepared_attempt,
            command_id: envelope.command_id,
            cancellation_id: envelope.cancellation_id,
            request_id: Some(host_request_id),
            issued_revision: self.revision,
            deadline_at_ms: Some(deadline),
            expected_selection_revision: runtime_plan.expected_selection_revision,
            max_attempts: TRANSCRIPT_WORKFLOW_MAX_ATTEMPTS,
            now_ms: now,
            expected_workflow_revision: force_retry_from_revision,
        });
        match outcome {
            Ok(TranscriptWorkflowEnsureOutcome::Changed(record))
            | Ok(TranscriptWorkflowEnsureOutcome::Existing(record)) => {
                if let Some(old) = existing
                    .as_ref()
                    .filter(|old| old.request_id != record.request_id)
                {
                    self.withdraw_transcript_request(old);
                }
                self.advance_revision();
                self.queue_transcript_request(&record);
                self.finish(envelope.command_id, OperationStage::Running, None, None);
            }
            Err(error) => self.fail(envelope.command_id, storage_failure(error)),
        }
    }

    fn authoritative_transcript_workflow_store(
        &mut self,
        envelope: &CommandEnvelope,
    ) -> Option<LibraryStore> {
        let Some(store) = self.store.clone() else {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return None;
        };
        match store.transcript_workflow_authority() {
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
}
