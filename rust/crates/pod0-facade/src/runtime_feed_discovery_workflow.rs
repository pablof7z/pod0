use pod0_application::{
    ApplicationCommand, CommandEnvelope, CoreWakeReason, DownloadIntentOrigin, HostRequest,
    HostRequestEnvelope, OperationStage,
};
use pod0_storage::{FeedDiscoveryEffectKind, FeedDiscoveryEffectRecord, FeedDiscoveryEffectStage};

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
        for request_id in store.expire_feed_discovery_notifications(now_ms)? {
            self.withdraw_feed_discovery_notification(request_id);
        }
        let _ = store.plan_pending_feed_discoveries(now_ms, 64)?;
        let requested_before = store
            .requested_feed_discovery_notifications(64)?
            .into_iter()
            .filter_map(|record| record.request_id)
            .collect::<std::collections::BTreeSet<_>>();
        let _ = store.reconcile_feed_discovery_preferences(now_ms)?;
        let requested_after = store
            .requested_feed_discovery_notifications(64)?
            .into_iter()
            .filter_map(|record| record.request_id)
            .collect::<std::collections::BTreeSet<_>>();
        for request_id in requested_before.difference(&requested_after) {
            self.withdraw_feed_discovery_notification(*request_id);
        }
        self.apply_feed_discovery_downloads(&store, now_ms)?;
        for record in store.requested_feed_discovery_notifications(64)? {
            self.queue_feed_discovery_notification(record)?;
        }
        let notification_effects = store.pending_feed_discovery_effects(
            FeedDiscoveryEffectKind::Notification,
            i64::MAX,
            64,
        )?;
        for record in notification_effects
            .iter()
            .filter(|record| record.stage == FeedDiscoveryEffectStage::RetryScheduled)
        {
            self.schedule_feed_discovery_retry(record);
        }
        for record in notification_effects.into_iter().filter(|record| {
            record.stage == FeedDiscoveryEffectStage::Pending
                || record.not_before_ms.is_none_or(|value| value <= now_ms)
        }) {
            let deadline = record.expires_at_ms;
            if deadline <= now_ms {
                continue;
            }
            if let Some(admitted) = store.admit_feed_discovery_notification(
                record.occurrence_id,
                record.episode_id,
                now_ms,
                deadline,
            )? {
                self.queue_feed_discovery_notification(admitted)?;
            }
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

    fn queue_feed_discovery_notification(
        &mut self,
        record: FeedDiscoveryEffectRecord,
    ) -> Result<bool, pod0_storage::StorageError> {
        let Some(request_id) = record.request_id else {
            return Ok(false);
        };
        if self
            .pending_feed_discovery_notifications
            .contains_key(&request_id)
        {
            return Ok(true);
        }
        let Some(command_id) = record.command_id else {
            return Ok(false);
        };
        let envelope = HostRequestEnvelope {
            request_id,
            command_id,
            cancellation_id: record.cancellation_id,
            issued_revision: record.workflow_revision,
            deadline_at: record
                .deadline_at_ms
                .map(pod0_domain::UnixTimestampMilliseconds::new),
            request: HostRequest::DeliverNewEpisodeNotification {
                occurrence_id: record.occurrence_id,
                episode_id: record.episode_id,
                podcast_id: record.podcast_id,
                podcast_title: record.podcast_title.clone(),
                episode_title: record.episode_title.clone(),
            },
        };
        if !self.host_requests.register(envelope.clone())
            && !self.host_requests.matches_outstanding(&envelope)
        {
            return Ok(false);
        }
        if !self
            .host_queue
            .iter()
            .any(|queued| queued.request_id == request_id)
        {
            self.host_queue.push_back(envelope);
        }
        self.pending_feed_discovery_notifications
            .insert(request_id, record);
        Ok(true)
    }

    pub(super) fn schedule_feed_discovery_retry(&mut self, record: &FeedDiscoveryEffectRecord) {
        let Some(wake_at_ms) = record.not_before_ms else {
            return;
        };
        let Some(command_id) = record.command_id else {
            return;
        };
        let _ = self.schedule_core_wake(
            command_id,
            record.cancellation_id,
            record.workflow_revision,
            wake_at_ms,
            CoreWakeReason::FeedDiscoveryNotificationRetry {
                occurrence_id: record.occurrence_id,
                episode_id: record.episode_id,
                attempt: record.attempt,
            },
        );
    }
}
