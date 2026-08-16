use pod0_application::{CommandEnvelope, HostRequest, OperationResult};

use crate::runtime_playback_state::PlaybackRuntime;
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn reset_all(&mut self, envelope: &CommandEnvelope, fingerprint: &str) {
        let active_episode_id = self.listening.playback.active_episode_id;
        let effects = active_episode_id.map_or_else(Vec::new, |episode_id| {
            vec![
                ("reset-stop", HostRequest::StopPlayback { episode_id }),
                ("reset-timer", HostRequest::CancelNativeTimer { episode_id }),
            ]
        });
        let Some(effects) = self.playback_effects(envelope, effects) else {
            self.fail(
                envelope.command_id,
                pod0_application::CoreFailureCode::InvalidCommand,
            );
            return;
        };
        let result = self
            .store
            .as_ref()
            .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
            .and_then(|store| {
                store.reset_listening_data_with_effects(
                    envelope.command_id,
                    fingerprint,
                    effects,
                    self.now().value,
                )
            });
        let succeeded = result.is_ok();
        self.finish_storage_command(envelope.command_id, result, OperationResult::ListeningReset);
        if succeeded {
            self.playback = PlaybackRuntime::default();
        }
    }
}
