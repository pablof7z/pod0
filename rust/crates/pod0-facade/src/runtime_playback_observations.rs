use pod0_application::{
    ApplicationCommand, CommandEnvelope, PlaybackCommand, PlaybackHostState,
    PlaybackLifecycleObservation, PlaybackPolicyState,
};
use pod0_domain::{CancellationId, CommandId, EpisodeId, HostRequestId};
use pod0_storage::PlaybackMutation;
use sha2::{Digest, Sha256};

use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn accept_playback_observation(
        &mut self,
        request_id: HostRequestId,
        cancellation_id: CancellationId,
        sequence_number: u64,
        observed_at_ms: i64,
        value: PlaybackLifecycleObservation,
    ) {
        if let Some(fence) = self.playback.position_command_fence_at_ms {
            if observed_at_ms < fence {
                return;
            }
            self.playback.position_command_fence_at_ms = None;
        }
        let active = self.listening.playback.active_episode_id;
        if value.episode_id.is_some() && value.episode_id != active {
            return;
        }
        self.playback.media_episode_id = value.episode_id;
        let prior = self.playback.last_observation.clone();
        let interruption_changed = prior
            .as_ref()
            .is_none_or(|previous| previous.interruption != value.interruption);
        let newly_ended = value.ended && prior.as_ref().is_none_or(|previous| !previous.ended);
        self.playback.host_state = value.state;
        self.playback.policy_state = policy_state(value.state, active.is_some());
        self.playback.last_observation = Some(value.clone());

        let reaction = reaction_envelope(request_id, cancellation_id, sequence_number);
        if interruption_changed && self.handle_interruption(&reaction, observed_at_ms, &value) {
            return;
        }

        let Some(episode_id) = active.filter(|id| value.episode_id == Some(*id)) else {
            return;
        };
        if self.playback.completion_checkpoint_fence_episode_id == Some(episode_id) {
            return;
        }
        let segment_reached = pod0_domain::segment_reached(
            value.position_milliseconds,
            self.listening.playback.active_segment,
        );
        let force_checkpoint = segment_reached || newly_ended;
        if !force_checkpoint
            && self.evaluate_automatic_ad_skip(
                &reaction,
                observed_at_ms,
                value.position_milliseconds,
            )
        {
            return;
        }
        self.checkpoint_observation(
            episode_id,
            value.position_milliseconds,
            observed_at_ms,
            force_checkpoint,
        );
        if segment_reached {
            self.finish_segment(&reaction, observed_at_ms, episode_id);
        } else if newly_ended {
            self.finish_episode(&reaction, observed_at_ms);
        }
    }

    pub(super) fn playback_host_failed(&mut self, command_id: CommandId) {
        self.playback.media_episode_id = None;
        self.playback.host_state = PlaybackHostState::Failed;
        self.playback.policy_state = PlaybackPolicyState::Failed;
        self.playback.desired_playing = false;
        self.fail(
            command_id,
            pod0_application::CoreFailureCode::HostUnavailable,
        );
    }

    pub(super) fn checkpoint_observation(
        &mut self,
        episode_id: EpisodeId,
        position_milliseconds: u64,
        observed_at_ms: i64,
        force: bool,
    ) {
        if position_milliseconds == 0 {
            return;
        }
        let Some(episode) = self
            .listening
            .episodes
            .iter()
            .find(|episode| episode.episode_id == episode_id)
        else {
            return;
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
            return;
        }
        let result = self
            .store
            .as_ref()
            .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
            .and_then(|store| {
                store.apply_playback_observation(
                    PlaybackMutation::Checkpoint {
                        episode_id,
                        position_milliseconds,
                    },
                    observed_at_ms,
                )
            });
        match result {
            Ok(_) => {
                self.playback.last_position_commit_at_ms = Some(observed_at_ms);
                if self.reload_listening().is_err() {
                    self.playback.policy_state = PlaybackPolicyState::Failed;
                }
            }
            Err(_) => self.playback.policy_state = PlaybackPolicyState::Failed,
        }
    }
}

fn policy_state(host: PlaybackHostState, has_active: bool) -> PlaybackPolicyState {
    match host {
        PlaybackHostState::Idle => {
            if has_active {
                PlaybackPolicyState::Paused
            } else {
                PlaybackPolicyState::Idle
            }
        }
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

fn reaction_envelope(
    request_id: HostRequestId,
    cancellation_id: CancellationId,
    sequence_number: u64,
) -> CommandEnvelope {
    let mut hash = Sha256::new();
    hash.update(b"pod0-playback-observation-reaction-v1\0");
    hash.update(request_id.into_bytes());
    hash.update(sequence_number.to_be_bytes());
    let digest = hash.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    CommandEnvelope {
        command_id: CommandId::from_bytes(bytes),
        cancellation_id,
        expected_revision: None,
        command: ApplicationCommand::Playback {
            command: PlaybackCommand::Restore,
        },
    }
}
