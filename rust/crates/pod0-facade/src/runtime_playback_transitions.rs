use pod0_application::{
    CommandEnvelope, HostRequest, PlaybackInterruption, PlaybackLifecycleObservation,
    PlaybackPolicyState, PlaybackTransitionCue,
};
use pod0_domain::EpisodeId;
use pod0_storage::PlaybackMutation;

use crate::runtime_state::FacadeState;
use crate::runtime_storage_commands::storage_failure;

impl FacadeState {
    pub(super) fn handle_interruption(
        &mut self,
        reaction: &CommandEnvelope,
        observed_at_ms: i64,
        value: &PlaybackLifecycleObservation,
    ) -> bool {
        let Some(episode_id) = self.listening.playback.active_episode_id else {
            return false;
        };
        match value.interruption {
            PlaybackInterruption::None => false,
            PlaybackInterruption::Began => {
                self.checkpoint_observation(
                    reaction,
                    episode_id,
                    value.position_milliseconds,
                    observed_at_ms,
                    true,
                );
                self.playback.interrupted_episode_id =
                    self.playback.desired_playing.then_some(episode_id);
                self.playback.policy_state = PlaybackPolicyState::Paused;
                self.issue_playback_request(
                    reaction,
                    "interruption-pause",
                    HostRequest::Pause { episode_id },
                );
                true
            }
            PlaybackInterruption::EndedShouldResume => {
                let should_resume = self.playback.desired_playing
                    && self.playback.interrupted_episode_id == Some(episode_id)
                    && !value.ended;
                self.playback.interrupted_episode_id = None;
                if should_resume {
                    self.playback.policy_state = PlaybackPolicyState::AwaitingHost;
                    self.issue_playback_request(
                        reaction,
                        "interruption-resume",
                        HostRequest::Play {
                            episode_id,
                            transition_cue: PlaybackTransitionCue::Immediate,
                        },
                    );
                } else {
                    self.playback.desired_playing = false;
                    self.playback.policy_state = PlaybackPolicyState::Paused;
                }
                true
            }
            PlaybackInterruption::EndedShouldRemainPaused | PlaybackInterruption::RouteLost => {
                self.checkpoint_observation(
                    reaction,
                    episode_id,
                    value.position_milliseconds,
                    observed_at_ms,
                    true,
                );
                self.playback.interrupted_episode_id = None;
                self.playback.desired_playing = false;
                self.playback.policy_state = PlaybackPolicyState::Paused;
                self.issue_playback_request(
                    reaction,
                    "boundary-pause",
                    HostRequest::Pause { episode_id },
                );
                true
            }
            PlaybackInterruption::MediaServicesReset => {
                self.checkpoint_observation(
                    reaction,
                    episode_id,
                    value.position_milliseconds,
                    observed_at_ms,
                    true,
                );
                self.playback.interrupted_episode_id = None;
                let resume = self.playback.desired_playing && !value.ended;
                self.load_active(reaction, resume, PlaybackTransitionCue::Immediate);
                true
            }
            PlaybackInterruption::Unsupported { .. } => {
                self.playback.desired_playing = false;
                self.playback.policy_state = PlaybackPolicyState::Failed;
                true
            }
        }
    }

    pub(super) fn finish_segment(
        &mut self,
        reaction: &CommandEnvelope,
        observed_at_ms: i64,
        prior_episode_id: EpisodeId,
    ) {
        let had_next = !self.listening.playback.queue.is_empty();
        if !self.apply_observation_mutation(
            reaction,
            "finish-segment",
            PlaybackMutation::AdvanceQueue,
            observed_at_ms,
        ) {
            return;
        }
        let next = self.listening.playback.active_episode_id;
        if had_next && next.is_some() {
            self.playback.desired_playing = true;
            self.load_active(
                reaction,
                true,
                PlaybackTransitionCue::FadeIn {
                    duration_milliseconds: 250,
                },
            );
        } else {
            self.playback.desired_playing = false;
            self.playback.policy_state = PlaybackPolicyState::Paused;
            self.issue_playback_request(
                reaction,
                "segment-pause",
                HostRequest::Pause {
                    episode_id: prior_episode_id,
                },
            );
        }
    }

    pub(super) fn finish_episode(&mut self, reaction: &CommandEnvelope, observed_at_ms: i64) {
        let should_advance = !self.listening.playback.queue.is_empty()
            && self.listening.playback.auto_play_next
            && self.listening.playback.sleep_mode != pod0_domain::PlaybackSleepMode::EndOfEpisode
            && !self.playback.timer_fired;
        let mutation = PlaybackMutation::FinishActive {
            suppress_auto_advance: self.playback.timer_fired,
        };
        if !self.apply_observation_mutation(reaction, "finish-episode", mutation, observed_at_ms) {
            return;
        }
        let next = self.listening.playback.active_episode_id;
        if should_advance && next.is_some() {
            self.playback.desired_playing = true;
            self.playback.timer_fired = false;
            self.load_active(
                reaction,
                true,
                PlaybackTransitionCue::FadeIn {
                    duration_milliseconds: 250,
                },
            );
        } else {
            self.playback.desired_playing = false;
            self.playback.policy_state = PlaybackPolicyState::Completed;
        }
    }

    fn apply_observation_mutation(
        &mut self,
        reaction: &CommandEnvelope,
        label: &str,
        mutation: PlaybackMutation,
        observed_at_ms: i64,
    ) -> bool {
        let result =
            self.commit_playback_observation_mutation(reaction, label, mutation, observed_at_ms);
        match result {
            Ok(_) => match self.reload_listening() {
                Ok(()) => true,
                Err(error) => {
                    self.playback.policy_state = PlaybackPolicyState::Failed;
                    let _ = storage_failure(error);
                    false
                }
            },
            Err(error) => {
                self.playback.policy_state = PlaybackPolicyState::Failed;
                let _ = storage_failure(error);
                false
            }
        }
    }

    pub(super) fn commit_playback_observation_mutation(
        &self,
        reaction: &CommandEnvelope,
        label: &str,
        mutation: PlaybackMutation,
        observed_at_ms: i64,
    ) -> Result<pod0_storage::PlaybackMutationResult, pod0_storage::StorageError> {
        let envelope = observation_action_envelope(reaction, label);
        let fingerprint =
            crate::runtime_command_fingerprint::command_fingerprint(&envelope.command);
        let episode_id = crate::runtime_playback_actions::playback_episode_hint(
            &mutation,
            self.listening.playback.active_episode_id,
        );
        let transition = crate::runtime_playback_actions::playback_transition(&mutation);
        self.store
            .as_ref()
            .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)?
            .apply_playback_mutation(
                envelope.command_id,
                &fingerprint,
                mutation,
                episode_id,
                transition,
                None,
                observed_at_ms,
            )
    }
}

fn observation_action_envelope(reaction: &CommandEnvelope, label: &str) -> CommandEnvelope {
    use sha2::{Digest as _, Sha256};
    let mut hash = Sha256::new();
    hash.update(b"pod0-playback-observation-action-v1\0");
    hash.update(reaction.command_id.into_bytes());
    hash.update(label.as_bytes());
    let digest: [u8; 32] = hash.finalize().into();
    CommandEnvelope {
        command_id: pod0_domain::CommandId::from_bytes(
            digest[..16].try_into().expect("fixed digest prefix"),
        ),
        cancellation_id: reaction.cancellation_id,
        expected_revision: None,
        command: pod0_application::ApplicationCommand::Playback {
            command: pod0_application::PlaybackCommand::Restore,
        },
    }
}
