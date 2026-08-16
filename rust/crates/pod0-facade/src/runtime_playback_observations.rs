use pod0_application::PlaybackHostState;
use pod0_domain::{CommandId, EpisodeId};
use pod0_storage::PlaybackMutation;

use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn playback_host_failed(&mut self, command_id: CommandId) {
        self.playback.media_episode_id = None;
        self.playback.host_state = PlaybackHostState::Failed;
        self.playback.policy_state = pod0_application::PlaybackPolicyState::Failed;
        self.playback.desired_playing = false;
        self.fail(
            command_id,
            pod0_application::CoreFailureCode::HostUnavailable,
        );
    }

    pub(super) fn checkpoint_mutation(
        &self,
        episode_id: EpisodeId,
        position_milliseconds: u64,
        observed_at_ms: i64,
        force: bool,
    ) -> PlaybackMutation {
        if position_milliseconds == 0 {
            return PlaybackMutation::ReceiptOnly;
        }
        let Some(episode) = self
            .listening
            .episodes
            .iter()
            .find(|episode| episode.episode_id == episode_id)
        else {
            return PlaybackMutation::ReceiptOnly;
        };
        let last = self
            .playback
            .last_position_commit_at_ms
            .map(pod0_domain::UnixTimestampMilliseconds::new);
        if !pod0_domain::should_commit_position(
            episode.listening.resume_position_milliseconds,
            position_milliseconds,
            last,
            pod0_domain::UnixTimestampMilliseconds::new(observed_at_ms),
            force,
        ) {
            return PlaybackMutation::ReceiptOnly;
        }
        PlaybackMutation::Checkpoint {
            episode_id,
            position_milliseconds,
        }
    }
}
