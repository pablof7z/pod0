use pod0_application::{
    ActivityDomain, ActivitySubject, CommandEnvelope, CoreFailureCode,
    DurableInternalCommandRequest, HostRequest, InternalCommandKind, OperationResult,
    PlaybackTransitionCue, TranscriptWorkflowConfiguration, TranscriptWorkflowOrigin,
};
use pod0_domain::{EpisodeId, PlaybackSleepMode};
use pod0_storage::PlaybackMutation;

use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn play(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        transcript_configuration: Option<TranscriptWorkflowConfiguration>,
    ) {
        let Some(episode_id) = self.listening.playback.active_episode_id else {
            self.fail(envelope.command_id, CoreFailureCode::NotFound);
            return;
        };
        let internal_command = transcript_configuration
            .filter(|_| {
                self.transcript_origin_is_allowed(episode_id, TranscriptWorkflowOrigin::Playback)
            })
            .map(|configuration| DurableInternalCommandRequest {
                kind: InternalCommandKind::EnsureTranscriptWorkflow {
                    origin: TranscriptWorkflowOrigin::Playback,
                    configuration,
                },
                target: ActivityDomain::Transcript,
                subject: ActivitySubject::Episode { episode_id },
                episode_id: Some(episode_id),
            });
        let must_reload = must_reload_for_play(self, episode_id);
        let effects = if must_reload {
            self.plan_active_load_effects(envelope, true, PlaybackTransitionCue::Immediate)
        } else {
            let mut requests = vec![(
                "play",
                HostRequest::Play {
                    episode_id,
                    transition_cue: PlaybackTransitionCue::Immediate,
                },
            )];
            self.append_playback_stream_request(&mut requests);
            self.playback_effects(envelope, requests)
        };
        let Some(effects) = effects else {
            self.fail(envelope.command_id, CoreFailureCode::InvalidCommand);
            return;
        };
        if self.apply_playback_command_with_internal_and_effects(
            envelope,
            fingerprint,
            PlaybackMutation::ReceiptOnly,
            OperationResult::PlaybackUpdated {
                episode_id: Some(episode_id),
            },
            internal_command,
            effects,
        ) {
            self.playback.completion_checkpoint_fence_episode_id = None;
            self.playback.desired_playing = true;
            self.playback.timer_fired = false;
            self.playback.policy_state = pod0_application::PlaybackPolicyState::AwaitingHost;
            self.resume_playback_transcript_commands();
            if must_reload {
                self.playback.media_episode_id = Some(episode_id);
            }
            self.note_playback_stream_authorized(envelope);
        }
    }

    pub(super) fn pause(&mut self, envelope: &CommandEnvelope, fingerprint: &str) {
        let Some(episode_id) = self.listening.playback.active_episode_id else {
            self.fail(envelope.command_id, CoreFailureCode::NotFound);
            return;
        };
        let mutation = self.latest_position_for(episode_id).map_or(
            PlaybackMutation::ReceiptOnly,
            |position_milliseconds| PlaybackMutation::Checkpoint {
                episode_id,
                position_milliseconds,
            },
        );
        if self.apply_playback_command_with_effects(
            envelope,
            fingerprint,
            mutation,
            OperationResult::PlaybackUpdated {
                episode_id: Some(episode_id),
            },
            vec![("pause", HostRequest::Pause { episode_id })],
        ) {
            self.playback.desired_playing = false;
            self.playback.policy_state = pod0_application::PlaybackPolicyState::Paused;
        }
    }

    pub(super) fn seek(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        position_milliseconds: u64,
    ) {
        let Some(episode_id) = self.listening.playback.active_episode_id else {
            self.fail(envelope.command_id, CoreFailureCode::NotFound);
            return;
        };
        let command_at_ms = self.now().value;
        if self.apply_playback_command_with_effects(
            envelope,
            fingerprint,
            PlaybackMutation::Checkpoint {
                episode_id,
                position_milliseconds,
            },
            OperationResult::PlaybackUpdated {
                episode_id: Some(episode_id),
            },
            vec![(
                "seek",
                HostRequest::Seek {
                    episode_id,
                    position_milliseconds,
                    reason: pod0_domain::PlaybackSeekReason::UserRequested,
                    chapter_context: None,
                },
            )],
        ) {
            self.playback.completion_checkpoint_fence_episode_id = None;
            self.playback.position_command_fence_at_ms = Some(command_at_ms);
            self.playback.last_position_commit_at_ms = Some(command_at_ms);
        }
    }

    pub(super) fn advance_queue(&mut self, envelope: &CommandEnvelope, fingerprint: &str) {
        if self.listening.playback.queue.is_empty() {
            self.apply_playback_command(
                envelope,
                fingerprint,
                PlaybackMutation::ReceiptOnly,
                OperationResult::QueueUpdated,
            );
            return;
        }
        let next = self.listening.playback.queue.first().cloned();
        let Some(next) = next else { return };
        let Some(effects) = self.plan_episode_load_effects(
            envelope,
            next.episode_id,
            next.segment,
            true,
            PlaybackTransitionCue::FadeIn {
                duration_milliseconds: 250,
            },
        ) else {
            self.fail(envelope.command_id, CoreFailureCode::NotFound);
            return;
        };
        if self.apply_playback_command_with_durable_effects(
            envelope,
            fingerprint,
            PlaybackMutation::AdvanceQueue,
            OperationResult::QueueUpdated,
            effects,
        ) {
            self.playback.completion_checkpoint_fence_episode_id = None;
            self.playback.desired_playing = self.listening.playback.active_episode_id.is_some();
            self.playback.media_episode_id = Some(next.episode_id);
            self.playback.policy_state = pod0_application::PlaybackPolicyState::AwaitingHost;
            self.note_playback_stream_authorized(envelope);
        }
    }

    pub(super) fn timer_fired(&mut self, envelope: &CommandEnvelope, fingerprint: &str) {
        let episode_id = self.listening.playback.active_episode_id;
        let effects = episode_id.map_or_else(Vec::new, |episode_id| {
            vec![("timer-pause", HostRequest::Pause { episode_id })]
        });
        if self.apply_playback_command_with_effects(
            envelope,
            fingerprint,
            PlaybackMutation::SetSleepTimer(PlaybackSleepMode::Off),
            OperationResult::PlaybackUpdated { episode_id },
            effects,
        ) {
            self.playback.timer_fired = true;
            self.playback.desired_playing = false;
            self.playback.policy_state = pod0_application::PlaybackPolicyState::Paused;
        }
    }

    fn latest_position_for(&self, episode_id: EpisodeId) -> Option<u64> {
        self.playback.last_observation.as_ref().and_then(|value| {
            (value.episode_id == Some(episode_id)).then_some(value.position_milliseconds)
        })
    }
}

fn must_reload_for_play(state: &FacadeState, episode_id: EpisodeId) -> bool {
    state.playback.media_episode_id != Some(episode_id)
        || state
            .playback
            .last_observation
            .as_ref()
            .is_some_and(|value| value.ended)
        || matches!(
            state.playback.host_state,
            pod0_application::PlaybackHostState::Failed
                | pod0_application::PlaybackHostState::Unsupported { .. }
        )
}
