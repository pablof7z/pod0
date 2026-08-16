use pod0_application::{ApplicationCommand, CommandEnvelope, DownloadIntentOrigin, OperationStage};
use pod0_storage::{FeedDiscoveryEffectKind, FeedDiscoveryEffectStage};

use crate::runtime_command_fingerprint::command_fingerprint;
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn rehydrate_feed_discovery_workflows(
        &mut self,
    ) -> Result<(), pod0_storage::StorageError> {
        self.reconcile_feed_discovery_workflows()
    }

    pub(super) fn reconcile_feed_discovery_workflows(
        &mut self,
    ) -> Result<(), pod0_storage::StorageError> {
        let Some(store) = self.store.clone() else {
            return Ok(());
        };
        let now_ms = self.now().value;
        let _ = store.plan_pending_feed_discoveries(now_ms, 64)?;
        let _ = store.reconcile_feed_discovery_preferences(now_ms)?;
        self.apply_feed_discovery_downloads(&store, now_ms)?;
        let notification_effects = store.pending_feed_discovery_effects(
            FeedDiscoveryEffectKind::Notification,
            i64::MAX,
            64,
        )?;
        for record in notification_effects.into_iter().filter(|record| {
            record.stage == FeedDiscoveryEffectStage::Pending
                || record.not_before_ms.is_none_or(|value| value <= now_ms)
        }) {
            let deadline = record.expires_at_ms;
            if deadline <= now_ms {
                continue;
            }
            let _ = store.admit_feed_discovery_notification(
                record.occurrence_id,
                record.episode_id,
                now_ms,
                deadline,
            )?;
        }
        Ok(())
    }

    fn apply_feed_discovery_downloads(
        &mut self,
        store: &pod0_storage::LibraryStore,
        now_ms: i64,
    ) -> Result<(), pod0_storage::StorageError> {
        let effects =
            store.pending_feed_discovery_effects(FeedDiscoveryEffectKind::Download, now_ms, 64)?;
        for record in effects {
            let Some(command_id) = record.command_id else {
                continue;
            };
            let child = CommandEnvelope {
                command_id,
                cancellation_id: record.cancellation_id,
                expected_revision: None,
                command: ApplicationCommand::RequestEpisodeDownload {
                    episode_id: record.episode_id,
                    origin: DownloadIntentOrigin::Automatic,
                },
            };
            self.begin(&child);
            let fingerprint = command_fingerprint(&child.command);
            self.request_episode_download(
                &child,
                &fingerprint,
                record.episode_id,
                DownloadIntentOrigin::Automatic,
            );
            let accepted = self
                .operations
                .iter()
                .rev()
                .find(|operation| operation.command_id == command_id)
                .is_some_and(|operation| operation.stage != OperationStage::Failed);
            if accepted {
                let _ = store.mark_feed_discovery_download_applied(
                    record.occurrence_id,
                    record.episode_id,
                    now_ms,
                )?;
            }
        }
        Ok(())
    }
}
