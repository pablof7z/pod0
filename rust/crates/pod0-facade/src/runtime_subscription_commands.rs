use pod0_application::{CommandEnvelope, OperationResult};
use pod0_domain::{AutoDownloadPolicy, PodcastId};

use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn unsubscribe_podcast(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        podcast_id: PodcastId,
    ) {
        let result = self
            .store
            .as_ref()
            .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
            .and_then(|store| {
                store.unsubscribe(
                    envelope.command_id,
                    fingerprint,
                    podcast_id,
                    self.now().value,
                )
            });
        self.finish_storage_command(
            envelope.command_id,
            result,
            OperationResult::RemovedPodcast { podcast_id },
        );
    }

    pub(super) fn set_subscription_notifications(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        podcast_id: PodcastId,
        enabled: bool,
    ) {
        let result = self
            .store
            .as_ref()
            .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
            .and_then(|store| {
                store.update_subscription_preferences(
                    envelope.command_id,
                    fingerprint,
                    podcast_id,
                    None,
                    Some(enabled),
                    self.now().value,
                )
            });
        self.finish_storage_command(
            envelope.command_id,
            result,
            OperationResult::PreferencesUpdated { podcast_id },
        );
    }

    pub(super) fn set_subscription_auto_download(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        podcast_id: PodcastId,
        policy: AutoDownloadPolicy,
    ) {
        let result = self
            .store
            .as_ref()
            .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
            .and_then(|store| {
                store.update_subscription_preferences(
                    envelope.command_id,
                    fingerprint,
                    podcast_id,
                    Some(policy),
                    None,
                    self.now().value,
                )
            });
        let preference_changed = result.is_ok();
        self.finish_storage_command(
            envelope.command_id,
            result,
            OperationResult::PreferencesUpdated { podcast_id },
        );
        if preference_changed {
            let _ = self.reconcile_download_admission();
        }
    }
}
