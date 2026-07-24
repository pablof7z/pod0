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
        let preference_changed = result.is_ok();
        self.finish_storage_command(
            envelope.command_id,
            result,
            OperationResult::RemovedPodcast { podcast_id },
        );
        if preference_changed {
            let _ = self.reconcile_feed_discovery_workflows();
        }
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
        let preference_changed = result.is_ok();
        self.finish_storage_command(
            envelope.command_id,
            result,
            OperationResult::PreferencesUpdated { podcast_id },
        );
        if preference_changed {
            let _ = self.reconcile_feed_discovery_workflows();
        }
    }

    pub(super) fn set_new_episode_notifications_enabled(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        enabled: bool,
    ) {
        let result = self
            .store
            .as_ref()
            .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
            .and_then(|store| {
                store.set_new_episode_notifications_enabled(
                    envelope.command_id,
                    fingerprint,
                    enabled,
                    self.now().value,
                )
            });
        match result {
            Ok(settings) => {
                self.revision = pod0_domain::StateRevision::new(
                    self.revision.value.max(settings.revision.value),
                );
                self.new_episode_notification_settings = settings;
                let _ = self.reconcile_feed_discovery_workflows();
                self.succeed(envelope.command_id, None);
            }
            Err(error) => {
                self.fail(
                    envelope.command_id,
                    crate::runtime_storage_commands::storage_failure(error),
                );
            }
        }
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
