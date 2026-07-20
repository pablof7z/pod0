use pod0_application::{
    CommandEnvelope, CoreFailureCode, HostRequest, HostRequestEnvelope, NativeTimerMode,
    OperationResult, PlaybackTransitionCue,
};
use pod0_domain::{CommandId, EpisodeId, HostRequestId, PlaybackSleepMode};
use pod0_storage::PlaybackMutation;
use sha2::{Digest, Sha256};

use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn set_sleep_timer(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        mode: PlaybackSleepMode,
    ) {
        if let PlaybackSleepMode::Unsupported { wire_code } = mode {
            self.fail(
                envelope.command_id,
                CoreFailureCode::Unsupported { wire_code },
            );
            return;
        }
        let episode_id = self.listening.playback.active_episode_id;
        if !self.apply_playback_command(
            envelope,
            fingerprint,
            PlaybackMutation::SetSleepTimer(mode),
            OperationResult::PlaybackUpdated { episode_id },
        ) {
            return;
        }
        self.playback.timer_fired = false;
        let Some(episode_id) = episode_id else { return };
        let request = match mode {
            PlaybackSleepMode::Off => HostRequest::CancelNativeTimer { episode_id },
            PlaybackSleepMode::Duration {
                duration_milliseconds,
            } => HostRequest::ArmNativeTimer {
                episode_id,
                mode: NativeTimerMode::Duration {
                    duration_milliseconds,
                },
            },
            PlaybackSleepMode::EndOfEpisode => HostRequest::ArmNativeTimer {
                episode_id,
                mode: NativeTimerMode::EndOfEpisode,
            },
            PlaybackSleepMode::Unsupported { .. } => unreachable!(),
        };
        self.issue_playback_request(envelope, "sleep", request);
    }

    pub(super) fn load_active(
        &mut self,
        envelope: &CommandEnvelope,
        play_after_load: bool,
        transition_cue: PlaybackTransitionCue,
    ) {
        let _ = self.sync_active_chapter(envelope.command_id);
        let Some(episode_id) = self.listening.playback.active_episode_id else {
            self.playback.policy_state = pod0_application::PlaybackPolicyState::Idle;
            return;
        };
        let Some(episode) = self
            .listening
            .episodes
            .iter()
            .find(|episode| episode.episode_id == episode_id)
        else {
            self.fail(envelope.command_id, CoreFailureCode::NotFound);
            return;
        };
        let start_position_milliseconds =
            pod0_domain::playback_start_position(episode, self.listening.playback.active_segment);
        let audio_url = episode.enclosure_url.clone();
        let rate = self.listening.playback.rate;
        self.playback.media_episode_id = Some(episode_id);
        self.playback.policy_state = pod0_application::PlaybackPolicyState::AwaitingHost;
        self.issue_playback_request(
            envelope,
            "load",
            HostRequest::LoadMedia {
                episode_id,
                audio_url,
                start_position_milliseconds,
            },
        );
        self.issue_playback_request(
            envelope,
            "load-rate",
            HostRequest::SetRate { episode_id, rate },
        );
        self.issue_timer_for_active(envelope, episode_id);
        if play_after_load {
            self.issue_playback_request(
                envelope,
                "load-play",
                HostRequest::Play {
                    episode_id,
                    transition_cue,
                },
            );
        }
        self.ensure_playback_stream(envelope);
    }

    fn issue_timer_for_active(&mut self, envelope: &CommandEnvelope, episode_id: EpisodeId) {
        let request = match self.listening.playback.sleep_mode {
            PlaybackSleepMode::Off => HostRequest::CancelNativeTimer { episode_id },
            PlaybackSleepMode::Duration {
                duration_milliseconds,
            } => HostRequest::ArmNativeTimer {
                episode_id,
                mode: NativeTimerMode::Duration {
                    duration_milliseconds,
                },
            },
            PlaybackSleepMode::EndOfEpisode => HostRequest::ArmNativeTimer {
                episode_id,
                mode: NativeTimerMode::EndOfEpisode,
            },
            PlaybackSleepMode::Unsupported { .. } => return,
        };
        self.issue_playback_request(envelope, "load-timer", request);
    }

    pub(super) fn ensure_playback_stream(&mut self, envelope: &CommandEnvelope) {
        if self.playback.observation_request_id.is_some() {
            return;
        }
        let request_id = self.issue_playback_request(
            envelope,
            "observe",
            HostRequest::ObservePlayback {
                episode_id: None,
                minimum_interval_milliseconds: 1_000,
            },
        );
        self.playback.observation_request_id = request_id;
    }

    pub(super) fn issue_playback_request(
        &mut self,
        envelope: &CommandEnvelope,
        tag: &str,
        request: HostRequest,
    ) -> Option<HostRequestId> {
        let request_id = playback_request_id(envelope.command_id, tag);
        let request = HostRequestEnvelope {
            request_id,
            command_id: envelope.command_id,
            cancellation_id: envelope.cancellation_id,
            issued_revision: self.revision,
            deadline_at: None,
            request,
        };
        if self.host_requests.register(request.clone()) {
            self.host_queue.push_back(request);
            Some(request_id)
        } else {
            None
        }
    }
}

fn playback_request_id(command_id: CommandId, tag: &str) -> HostRequestId {
    let mut hash = Sha256::new();
    hash.update(b"pod0-playback-host-request-v1\0");
    hash.update(command_id.into_bytes());
    hash.update(tag.as_bytes());
    let digest = hash.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    HostRequestId::from_bytes(bytes)
}
