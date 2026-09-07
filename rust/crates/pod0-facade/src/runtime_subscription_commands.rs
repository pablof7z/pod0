use pod0_application::{CommandEnvelope, OperationResult};
use pod0_domain::{
    AutoDownloadPolicy, CancellationId, CommandId, ContentDigest, PodcastId, TranscriptStartPolicy,
};
use sha2::{Digest as _, Sha256};

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
                if !enabled {
                    self.withdraw_feed_notifications(envelope.command_id);
                }
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

    fn withdraw_feed_notifications(&mut self, parent_command_id: CommandId) {
        let Some(store) = self.store.clone() else {
            return;
        };
        let Ok(records) = store.requested_feed_discovery_notifications(64) else {
            return;
        };
        for record in records {
            let (command_id, fingerprint) =
                notification_withdrawal_identity(parent_command_id, record.cancellation_id);
            if store
                .cancel_durable_effects(command_id, fingerprint, record.cancellation_id, self.now())
                .is_ok()
            {
                self.host_requests.cancel(record.cancellation_id);
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

    pub(super) fn set_subscription_transcript_start_policy(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        podcast_id: PodcastId,
        policy: TranscriptStartPolicy,
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
                    None,
                    Some(policy),
                    self.now().value,
                )
            });
        self.finish_storage_command(
            envelope.command_id,
            result,
            OperationResult::PreferencesUpdated { podcast_id },
        );
    }
}

fn notification_withdrawal_identity(
    parent_command_id: CommandId,
    cancellation_id: CancellationId,
) -> (CommandId, ContentDigest) {
    let mut hash = Sha256::new();
    hash.update(b"pod0/feed-notification-withdrawal/v1\0");
    hash.update(parent_command_id.into_bytes());
    hash.update(cancellation_id.into_bytes());
    let digest: [u8; 32] = hash.finalize().into();
    (
        CommandId::from_bytes(digest[..16].try_into().expect("digest prefix")),
        ContentDigest::from_bytes(digest),
    )
}
