use pod0_application::{
    CommandEnvelope, HostRequest, PlaybackInterruption, PlaybackLifecycleObservation,
    PlaybackPolicyState, PlaybackTransitionCue,
};
use pod0_domain::EpisodeId;
use pod0_storage::PlaybackMutation;

use crate::runtime_commands::storage_failure;
use crate::runtime_state::FacadeState;

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
        if !self.apply_observation_mutation(PlaybackMutation::AdvanceQueue, observed_at_ms) {
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
        if !self.apply_observation_mutation(mutation, observed_at_ms) {
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
        mutation: PlaybackMutation,
        observed_at_ms: i64,
    ) -> bool {
        let result = self
            .store
            .as_ref()
            .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
            .and_then(|store| store.apply_playback_observation(mutation, observed_at_ms));
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
}
