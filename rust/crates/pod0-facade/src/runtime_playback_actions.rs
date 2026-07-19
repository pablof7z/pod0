use pod0_application::{
    CommandEnvelope, CoreFailureCode, HostRequest, OperationResult, PlaybackTransitionCue,
};
use pod0_domain::{EpisodeId, PlaybackSleepMode};
use pod0_storage::PlaybackMutation;

use crate::runtime_commands::storage_failure;
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn play(&mut self, envelope: &CommandEnvelope, fingerprint: &str) {
        let Some(episode_id) = self.listening.playback.active_episode_id else {
            self.fail(envelope.command_id, CoreFailureCode::NotFound);
            return;
        };
        if self.apply_playback_command(
            envelope,
            fingerprint,
            PlaybackMutation::ReceiptOnly,
            OperationResult::PlaybackUpdated {
                episode_id: Some(episode_id),
            },
        ) {
            self.playback.desired_playing = true;
            self.playback.timer_fired = false;
            self.playback.policy_state = pod0_application::PlaybackPolicyState::AwaitingHost;
            let must_reload = self.playback.media_episode_id != Some(episode_id)
                || self
                    .playback
                    .last_observation
                    .as_ref()
                    .is_some_and(|value| value.ended)
                || matches!(
                    self.playback.host_state,
                    pod0_application::PlaybackHostState::Failed
                        | pod0_application::PlaybackHostState::Unsupported { .. }
                );
            if must_reload {
                self.load_active(envelope, true, PlaybackTransitionCue::Immediate);
            } else {
                self.issue_playback_request(
                    envelope,
                    "play",
                    HostRequest::Play {
                        episode_id,
                        transition_cue: PlaybackTransitionCue::Immediate,
                    },
                );
                self.ensure_playback_stream(envelope);
            }
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
        if self.apply_playback_command(
            envelope,
            fingerprint,
            mutation,
            OperationResult::PlaybackUpdated {
                episode_id: Some(episode_id),
            },
        ) {
            self.playback.desired_playing = false;
            self.playback.policy_state = pod0_application::PlaybackPolicyState::Paused;
            self.issue_playback_request(envelope, "pause", HostRequest::Pause { episode_id });
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
        if self.apply_playback_command(
            envelope,
            fingerprint,
            PlaybackMutation::Checkpoint {
                episode_id,
                position_milliseconds,
            },
            OperationResult::PlaybackUpdated {
                episode_id: Some(episode_id),
            },
        ) {
            self.playback.position_command_fence_at_ms = Some(command_at_ms);
            self.playback.last_position_commit_at_ms = Some(command_at_ms);
            self.issue_playback_request(
                envelope,
                "seek",
                HostRequest::Seek {
                    episode_id,
                    position_milliseconds,
                },
            );
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
        if self.apply_playback_command(
            envelope,
            fingerprint,
            PlaybackMutation::AdvanceQueue,
            OperationResult::QueueUpdated,
        ) {
            self.playback.desired_playing = self.listening.playback.active_episode_id.is_some();
            self.load_active(
                envelope,
                true,
                PlaybackTransitionCue::FadeIn {
                    duration_milliseconds: 250,
                },
            );
        }
    }

    pub(super) fn timer_fired(&mut self, envelope: &CommandEnvelope, fingerprint: &str) {
        let episode_id = self.listening.playback.active_episode_id;
        if self.apply_playback_command(
            envelope,
            fingerprint,
            PlaybackMutation::SetSleepTimer(PlaybackSleepMode::Off),
            OperationResult::PlaybackUpdated { episode_id },
        ) {
            self.playback.timer_fired = true;
            self.playback.desired_playing = false;
            self.playback.policy_state = pod0_application::PlaybackPolicyState::Paused;
            if let Some(episode_id) = episode_id {
                self.issue_playback_request(
                    envelope,
                    "timer-pause",
                    HostRequest::Pause { episode_id },
                );
            }
        }
    }

    fn latest_position_for(&self, episode_id: EpisodeId) -> Option<u64> {
        self.playback.last_observation.as_ref().and_then(|value| {
            (value.episode_id == Some(episode_id)).then_some(value.position_milliseconds)
        })
    }

    pub(super) fn apply_playback_command(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        mutation: PlaybackMutation,
        result: OperationResult,
    ) -> bool {
        let outcome = self
            .store
            .as_ref()
            .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
            .and_then(|store| {
                store.apply_playback_mutation(
                    envelope.command_id,
                    fingerprint,
                    mutation,
                    self.now().value,
                )
            });
        match outcome {
            Ok(_) => match self.reload_listening() {
                Ok(()) => {
                    self.succeed(envelope.command_id, Some(result));
                    true
                }
                Err(error) => {
                    self.fail(envelope.command_id, storage_failure(error));
                    false
                }
            },
            Err(error) => {
                self.fail(envelope.command_id, storage_failure(error));
                false
            }
        }
    }
}
