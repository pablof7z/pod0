use pod0_application::{
    InternalCommandKind, RequestDisposition, RequestRejectionReason,
    TRANSCRIPT_HOST_REQUEST_DEADLINE_MILLISECONDS, TRANSCRIPT_WORKFLOW_MAX_ATTEMPTS,
    TranscriptGenerationDecision, TranscriptWorkflowOrigin, transcript_attempt_id,
    transcript_submission_fence_id,
};
use pod0_domain::{CancellationId, CommandId, EpisodeId, InternalCommandId, TranscriptStartPolicy};
use pod0_storage::{
    LibraryStore, PendingInternalCommand, PreparedTranscriptAttempt, StoredTranscriptWorkflowStage,
    TranscriptWorkflowEnsureInput, TranscriptWorkflowEnsureOutcome,
};
use sha2::{Digest as _, Sha256};

use crate::runtime_state::FacadeState;
use crate::runtime_transcript_workflow_mapping::{request_id, stored_request};

impl FacadeState {
    pub(super) fn transcript_origin_is_allowed(
        &self,
        episode_id: EpisodeId,
        origin: TranscriptWorkflowOrigin,
    ) -> bool {
        if origin == TranscriptWorkflowOrigin::User {
            return true;
        }
        let Some(episode) = self
            .listening
            .episodes
            .iter()
            .find(|episode| episode.episode_id == episode_id)
        else {
            return false;
        };
        let policy = self
            .listening
            .subscriptions
            .iter()
            .find(|subscription| subscription.podcast_id == episode.podcast_id)
            .map_or(TranscriptStartPolicy::Automatic, |subscription| {
                subscription.transcript_start_policy
            });
        matches!(
            (origin, policy),
            (
                TranscriptWorkflowOrigin::Automatic,
                TranscriptStartPolicy::Automatic
            ) | (
                TranscriptWorkflowOrigin::Playback,
                TranscriptStartPolicy::WhenPlayed
            )
        )
    }

    pub(super) fn resume_playback_transcript_commands(&mut self) {
        let Some(store) = self.store.clone() else {
            return;
        };
        if !store
            .transcript_workflow_authority()
            .is_ok_and(|authority| authority.is_authoritative())
        {
            return;
        }
        let Ok(commands) = store.pending_internal_commands(100) else {
            return;
        };
        for command in commands {
            if matches!(
                command.request.kind,
                InternalCommandKind::EnsureTranscriptWorkflow { .. }
            ) {
                self.execute_playback_transcript_command(&store, command);
            }
        }
    }

    fn execute_playback_transcript_command(
        &mut self,
        store: &LibraryStore,
        command: PendingInternalCommand,
    ) {
        let InternalCommandKind::EnsureTranscriptWorkflow {
            origin,
            configuration,
        } = command.request.kind.clone()
        else {
            return;
        };
        let Some(episode_id) = command.request.episode_id else {
            return;
        };
        if !self.transcript_origin_is_allowed(episode_id, origin) {
            self.consume_transcript_command(
                store,
                command,
                episode_id,
                RequestDisposition::Rejected {
                    reason: RequestRejectionReason::NotAllowed,
                },
            );
            return;
        }
        let Ok(existing) = store.transcript_workflow(episode_id) else {
            return;
        };
        if existing.is_some() {
            self.consume_transcript_command(
                store,
                command,
                episode_id,
                RequestDisposition::Duplicate,
            );
            return;
        }
        let Some(runtime_plan) = self.transcript_workflow_plan(episode_id, origin, configuration)
        else {
            self.consume_transcript_command(
                store,
                command,
                episode_id,
                RequestDisposition::Rejected {
                    reason: RequestRejectionReason::MissingSubject,
                },
            );
            return;
        };
        let request = match runtime_plan.plan.generation {
            TranscriptGenerationDecision::Ensure => runtime_plan.plan.request,
            TranscriptGenerationDecision::Current => {
                self.consume_transcript_command(
                    store,
                    command,
                    episode_id,
                    RequestDisposition::AlreadyComplete,
                );
                return;
            }
            TranscriptGenerationDecision::AwaitingCredential { .. }
            | TranscriptGenerationDecision::AwaitingLocalAudio => {
                self.consume_transcript_command(
                    store,
                    command,
                    episode_id,
                    RequestDisposition::Rejected {
                        reason: RequestRejectionReason::MissingPrerequisite,
                    },
                );
                return;
            }
            TranscriptGenerationDecision::Blocked { .. } => {
                self.consume_transcript_command(
                    store,
                    command,
                    episode_id,
                    RequestDisposition::Rejected {
                        reason: RequestRejectionReason::Invalid,
                    },
                );
                return;
            }
            TranscriptGenerationDecision::NotRequested => {
                self.consume_transcript_command(
                    store,
                    command,
                    episode_id,
                    RequestDisposition::Rejected {
                        reason: RequestRejectionReason::NotAllowed,
                    },
                );
                return;
            }
        };
        let Some(request) = request else { return };
        let now = self.now().value;
        let attempt = existing
            .as_ref()
            .map_or(1, |value| value.attempt.saturating_add(1));
        let publisher = request.publisher_first;
        let prepared_attempt = (!publisher)
            .then(|| transcript_attempt_id(request.workflow_id, attempt))
            .flatten()
            .map(|attempt_id| PreparedTranscriptAttempt {
                attempt,
                attempt_id,
                submission_fence_id: transcript_submission_fence_id(attempt_id),
            });
        let outcome = store.ensure_transcript_workflow_from_internal_command(
            command.clone(),
            TranscriptWorkflowEnsureInput {
                episode_id,
                request: stored_request(request.clone()),
                stage: if publisher {
                    StoredTranscriptWorkflowStage::PublisherRequested
                } else {
                    StoredTranscriptWorkflowStage::Requested
                },
                prepared_attempt,
                command_id: CommandId::from_bytes(command.internal_command_id.into_bytes()),
                cancellation_id: internal_cancellation_id(command.internal_command_id),
                request_id: Some(request_id(request.workflow_id, attempt, publisher)),
                issued_revision: self.revision,
                deadline_at_ms: Some(
                    now.saturating_add(TRANSCRIPT_HOST_REQUEST_DEADLINE_MILLISECONDS),
                ),
                expected_selection_revision: runtime_plan.expected_selection_revision,
                max_attempts: TRANSCRIPT_WORKFLOW_MAX_ATTEMPTS,
                now_ms: now,
                expected_workflow_revision: None,
            },
        );
        if let Ok(
            TranscriptWorkflowEnsureOutcome::Changed(record)
            | TranscriptWorkflowEnsureOutcome::Existing(record),
        ) = outcome
        {
            if let Some(old) = existing
                .as_ref()
                .filter(|old| old.request_id != record.request_id)
            {
                self.withdraw_transcript_request(old);
            }
            self.advance_revision();
            self.queue_transcript_request(&record);
        }
    }

    fn consume_transcript_command(
        &self,
        store: &LibraryStore,
        command: PendingInternalCommand,
        episode_id: EpisodeId,
        disposition: RequestDisposition,
    ) {
        let _ = store.record_transcript_internal_disposition(
            command,
            episode_id,
            self.revision,
            disposition,
            self.now(),
        );
    }
}

fn internal_cancellation_id(command: InternalCommandId) -> CancellationId {
    let mut hash = Sha256::new();
    hash.update(b"pod0/playback-transcript/internal-cancellation/v1");
    hash.update(command.into_bytes());
    CancellationId::from_bytes(hash.finalize()[..16].try_into().expect("digest prefix"))
}
