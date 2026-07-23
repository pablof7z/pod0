use pod0_application::{CommandEnvelope, OperationResult};
use pod0_domain::EpisodeId;

use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn set_episode_starred(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        episode_id: EpisodeId,
        starred: bool,
    ) {
        let result = self
            .store
            .as_ref()
            .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
            .and_then(|store| {
                store.set_episode_starred(
                    envelope.command_id,
                    fingerprint,
                    episode_id,
                    starred,
                    self.now().value,
                )
            });
        self.finish_storage_command(
            envelope.command_id,
            result,
            OperationResult::EpisodeUpdated { episode_id },
        );
    }
}
