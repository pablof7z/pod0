use pod0_application::{
    CommandEnvelope, CoreFailureCode, HostRequest, HostRequestEnvelope, NativeTimerMode,
    OperationResult, PlaybackTransitionCue,
};
use pod0_domain::{CommandId, EpisodeId, HostRequestId, PlaybackSleepMode};
use pod0_storage::PlaybackMutation;
use sha2::{Digest, Sha256};

use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn playback_effects(
        &self,
        envelope: &CommandEnvelope,
        requests: Vec<(&str, HostRequest)>,
    ) -> Option<Vec<pod0_application::DurablePlaybackEffectRequest>> {
        requests
            .into_iter()
            .map(|(tag, request)| {
                pod0_application::DurablePlaybackEffectRequest::from_host(HostRequestEnvelope {
                    request_id: playback_request_id(envelope.command_id, tag),
                    command_id: envelope.command_id,
                    cancellation_id: envelope.cancellation_id,
                    issued_revision: self.revision,
                    deadline_at: None,
                    request,
                })
            })
            .collect()
    }

    pub(super) fn append_playback_stream_request<'a>(
        &self,
        requests: &mut Vec<(&'a str, HostRequest)>,
    ) {
        if self.playback.observation_request_id.is_none() {
            requests.push((
                "observe",
                HostRequest::ObservePlayback {
                    episode_id: None,
                    minimum_interval_milliseconds: 1_000,
                },
            ));
        }
    }

    pub(super) fn note_playback_stream_authorized(&mut self, envelope: &CommandEnvelope) {
        if self.playback.observation_request_id.is_none() {
            self.playback.observation_request_id =
                Some(playback_request_id(envelope.command_id, "observe"));
        }
    }

    pub(super) fn plan_active_load_effects(
        &mut self,
        envelope: &CommandEnvelope,
        play_after_load: bool,
        transition_cue: PlaybackTransitionCue,
    ) -> Option<Vec<pod0_application::DurablePlaybackEffectRequest>> {
        self.sync_active_chapter(envelope.command_id).ok()?;
        let episode_id = self.listening.playback.active_episode_id?;
        self.plan_episode_load_effects(
            envelope,
            episode_id,
            self.listening.playback.active_segment,
            play_after_load,
            transition_cue,
        )
    }

    pub(super) fn plan_episode_load_effects(
        &self,
        envelope: &CommandEnvelope,
        episode_id: EpisodeId,
        segment: Option<pod0_domain::PlaybackSegment>,
        play_after_load: bool,
        transition_cue: PlaybackTransitionCue,
    ) -> Option<Vec<pod0_application::DurablePlaybackEffectRequest>> {
        let episode = self
            .listening
            .episodes
            .iter()
            .find(|episode| episode.episode_id == episode_id)?;
        let mut requests = vec![
            (
                "load",
                HostRequest::LoadMedia {
                    episode_id,
                    audio_url: episode.enclosure_url.clone(),
                    start_position_milliseconds: pod0_domain::playback_start_position(
                        episode, segment,
                    ),
                },
            ),
            (
                "load-rate",
                HostRequest::SetRate {
                    episode_id,
                    rate: self.listening.playback.rate,
                },
            ),
            (
                "load-timer",
                timer_request(self.listening.playback.sleep_mode, episode_id)?,
            ),
        ];
        if play_after_load {
            requests.push((
                "load-play",
                HostRequest::Play {
                    episode_id,
                    transition_cue,
                },
            ));
        }
        self.append_playback_stream_request(&mut requests);
        self.playback_effects(envelope, requests)
    }

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
        let effects = episode_id
            .and_then(|episode_id| timer_request(mode, episode_id))
            .map_or_else(Vec::new, |request| vec![("sleep", request)]);
        if self.apply_playback_command_with_effects(
            envelope,
            fingerprint,
            PlaybackMutation::SetSleepTimer(mode),
            OperationResult::PlaybackUpdated { episode_id },
            effects,
        ) {
            self.playback.timer_fired = false;
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

fn timer_request(mode: PlaybackSleepMode, episode_id: EpisodeId) -> Option<HostRequest> {
    Some(match mode {
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
        PlaybackSleepMode::Unsupported { .. } => return None,
    })
}
