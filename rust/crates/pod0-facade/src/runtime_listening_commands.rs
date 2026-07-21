use pod0_application::{CommandEnvelope, HostRequest, OperationResult};

use crate::runtime_playback_state::PlaybackRuntime;
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn reset_listening_data(&mut self, envelope: &CommandEnvelope, fingerprint: &str) {
        let observation_request_id = self.playback.observation_request_id;
        let active_episode_id = self.listening.playback.active_episode_id;
        let result = self
            .store
            .as_ref()
            .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
            .and_then(|store| {
                store.reset_listening_data(envelope.command_id, fingerprint, self.now().value)
            });
        let succeeded = result.is_ok();
        self.finish_storage_command(envelope.command_id, result, OperationResult::ListeningReset);
        if succeeded {
            if let Some(episode_id) = active_episode_id {
                self.issue_playback_request(
                    envelope,
                    "reset-stop",
                    HostRequest::StopPlayback { episode_id },
                );
                self.issue_playback_request(
                    envelope,
                    "reset-timer",
                    HostRequest::CancelNativeTimer { episode_id },
                );
            }
            self.playback = PlaybackRuntime {
                observation_request_id,
                ..PlaybackRuntime::default()
            };
        }
    }
}
