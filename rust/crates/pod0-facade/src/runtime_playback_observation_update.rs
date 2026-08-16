use pod0_application::{PlaybackHostState, PlaybackLifecycleObservation, PlaybackPolicyState};

use crate::runtime_playback_observation_reaction::{
    PlannedPlaybackObservation, PlaybackObservationUpdate,
};
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn apply_playback_observation_update(
        &mut self,
        value: PlaybackLifecycleObservation,
        observed_at_ms: i64,
        plan: PlannedPlaybackObservation,
    ) {
        let update = plan.update;
        if matches!(update, PlaybackObservationUpdate::Ignore) {
            return;
        }
        self.playback.position_command_fence_at_ms = None;
        self.playback.media_episode_id = value.episode_id;
        self.playback.host_state = value.state;
        self.playback.policy_state = observed_policy_state(
            value.state,
            self.listening.playback.active_episode_id.is_some(),
        );
        self.playback.last_observation = Some(value);
        match update {
            PlaybackObservationUpdate::Ignore | PlaybackObservationUpdate::Base => {}
            PlaybackObservationUpdate::InterruptionBegan { checkpointed } => {
                note_checkpoint(self, checkpointed, observed_at_ms);
                self.playback.interrupted_episode_id = self
                    .playback
                    .desired_playing
                    .then_some(self.listening.playback.active_episode_id)
                    .flatten();
                self.playback.policy_state = PlaybackPolicyState::Paused;
            }
            PlaybackObservationUpdate::InterruptionResumed { resumed } => {
                self.playback.interrupted_episode_id = None;
                if resumed {
                    self.playback.policy_state = PlaybackPolicyState::AwaitingHost;
                } else {
                    self.playback.desired_playing = false;
                    self.playback.policy_state = PlaybackPolicyState::Paused;
                }
            }
            PlaybackObservationUpdate::InterruptionPaused { checkpointed } => {
                note_checkpoint(self, checkpointed, observed_at_ms);
                self.playback.interrupted_episode_id = None;
                self.playback.desired_playing = false;
                self.playback.policy_state = PlaybackPolicyState::Paused;
            }
            PlaybackObservationUpdate::MediaServicesReset {
                checkpointed,
                episode_id,
            } => {
                note_checkpoint(self, checkpointed, observed_at_ms);
                self.playback.interrupted_episode_id = None;
                self.playback.media_episode_id = Some(episode_id);
                self.playback.policy_state = PlaybackPolicyState::AwaitingHost;
            }
            PlaybackObservationUpdate::AutomaticAdSkip { ad_span_id } => {
                self.playback.skipped_ad_span_ids.insert(ad_span_id);
                self.playback.position_command_fence_at_ms = Some(observed_at_ms);
                self.playback.last_position_commit_at_ms = Some(observed_at_ms);
            }
            PlaybackObservationUpdate::SegmentFinished { checkpointed, next } => {
                note_checkpoint(self, checkpointed, observed_at_ms);
                if let Some(next) = next {
                    self.playback.desired_playing = true;
                    self.playback.media_episode_id = Some(next);
                    self.playback.policy_state = PlaybackPolicyState::AwaitingHost;
                } else {
                    self.playback.desired_playing = false;
                    self.playback.policy_state = PlaybackPolicyState::Paused;
                }
            }
            PlaybackObservationUpdate::EpisodeFinished { next } => {
                if let Some(next) = next {
                    self.playback.desired_playing = true;
                    self.playback.timer_fired = false;
                    self.playback.media_episode_id = Some(next);
                    self.playback.policy_state = PlaybackPolicyState::AwaitingHost;
                } else {
                    self.playback.desired_playing = false;
                    self.playback.policy_state = PlaybackPolicyState::Completed;
                }
            }
            PlaybackObservationUpdate::Checkpoint { committed } => {
                note_checkpoint(self, committed, observed_at_ms);
            }
            PlaybackObservationUpdate::Unsupported => {
                self.playback.desired_playing = false;
                self.playback.policy_state = PlaybackPolicyState::Failed;
            }
        }
    }
}

fn note_checkpoint(state: &mut FacadeState, committed: bool, observed_at_ms: i64) {
    if committed {
        state.playback.last_position_commit_at_ms = Some(observed_at_ms);
    }
}

fn observed_policy_state(host: PlaybackHostState, has_active: bool) -> PlaybackPolicyState {
    match host {
        PlaybackHostState::Idle if has_active => PlaybackPolicyState::Paused,
        PlaybackHostState::Idle => PlaybackPolicyState::Idle,
        PlaybackHostState::Loading | PlaybackHostState::Buffering => {
            PlaybackPolicyState::AwaitingHost
        }
        PlaybackHostState::Prepared | PlaybackHostState::Paused => PlaybackPolicyState::Paused,
        PlaybackHostState::Playing => PlaybackPolicyState::Playing,
        PlaybackHostState::Failed | PlaybackHostState::Unsupported { .. } => {
            PlaybackPolicyState::Failed
        }
    }
}
