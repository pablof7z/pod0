use pod0_application::{
    FEED_DISCOVERY_NOTIFICATION_MAX_ATTEMPTS, FEED_DISCOVERY_NOTIFICATION_RETRY_MILLISECONDS,
    FEED_DISCOVERY_NOTIFICATION_TTL_MILLISECONDS, MAX_NEW_EPISODE_NOTIFICATIONS_PER_OCCURRENCE,
    NewEpisodeNotificationSettingsProjection,
};
use pod0_domain::{
    AutoDownloadMode, CancellationId, CommandId, EpisodeId, FeedDiscoveryOccurrenceId,
    HostRequestId, PodcastId, StateRevision,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::StorageError;
use crate::feed_discovery_workflow_model::{
    FeedDiscoveryEffectKind, FeedDiscoveryEffectRecord, FeedDiscoveryEffectStage,
    FeedDiscoveryNotificationOutcome,
};
use crate::library_store::{LibraryStore, command_was_applied, finish_command};
use crate::listening_db_codec::decode_auto_download;

impl LibraryStore {
    pub fn plan_pending_feed_discoveries(
        &self,
        now_ms: i64,
        maximum_count: u16,
    ) -> Result<usize, StorageError> {
        self.write(|transaction| {
            plan_pending(transaction, now_ms, i64::from(maximum_count.clamp(1, 64)))
        })
    }

    pub fn reconcile_feed_discovery_preferences(&self, now_ms: i64) -> Result<usize, StorageError> {
        self.write(|transaction| {
            let changed = transaction
                .execute(
                    "UPDATE pod0_feed_discovery_effects AS effect
                     SET stage='obsolete',request_id=NULL,deadline_at_ms=NULL,
                         not_before_ms=NULL,
                         failure_code=CASE WHEN EXISTS(
                           SELECT 1 FROM pod0_feed_discovery_workflows workflow
                           WHERE workflow.occurrence_id=effect.occurrence_id
                             AND workflow.expires_at_ms<=?1
                         ) THEN 'expired' ELSE 'preference_disabled' END,
                         updated_at_ms=?1
                     WHERE effect.kind='notification'
                       AND effect.stage IN ('pending','requested','retry_scheduled')
                       AND (
                         (SELECT enabled FROM pod0_new_episode_notification_settings
                          WHERE singleton=1)=0
                         OR COALESCE((
                           SELECT subscription.notifications_enabled
                           FROM pod0_feed_discovery_occurrences occurrence
                           LEFT JOIN pod0_subscriptions subscription
                             ON subscription.podcast_id=occurrence.podcast_id
                           WHERE occurrence.occurrence_id=effect.occurrence_id
                         ),0)=0
                         OR EXISTS(
                           SELECT 1 FROM pod0_feed_discovery_workflows workflow
                           WHERE workflow.occurrence_id=effect.occurrence_id
                             AND workflow.expires_at_ms<=?1
                         )
                       )",
                    [now_ms],
                )
                .map_err(|error| {
                    StorageError::sqlite("reconcile notification preferences", error)
                })?;
            complete_workflows(transaction, now_ms)?;
            Ok(changed)
        })
    }

    pub fn pending_feed_discovery_effects(
        &self,
        kind: FeedDiscoveryEffectKind,
        now_ms: i64,
        maximum_count: u16,
    ) -> Result<Vec<FeedDiscoveryEffectRecord>, StorageError> {
        self.read(|connection| {
            read_effects(
                connection,
                kind,
                now_ms,
                i64::from(maximum_count.clamp(1, 64)),
            )
        })
    }

    pub fn requested_feed_discovery_notifications(
        &self,
        maximum_count: u16,
    ) -> Result<Vec<FeedDiscoveryEffectRecord>, StorageError> {
        self.read(|connection| {
            read_requested_notification_effects(connection, i64::from(maximum_count.clamp(1, 64)))
        })
    }

    pub fn mark_feed_discovery_download_applied(
        &self,
        occurrence_id: FeedDiscoveryOccurrenceId,
        episode_id: EpisodeId,
        now_ms: i64,
    ) -> Result<bool, StorageError> {
        self.write(|transaction| {
            let changed = transaction
                .execute(
                    "UPDATE pod0_feed_discovery_effects
                     SET stage='succeeded',updated_at_ms=?1
                     WHERE occurrence_id=?2 AND episode_id=?3 AND kind='download'
                       AND stage='pending'",
                    params![
                        now_ms,
                        occurrence_id.into_bytes().as_slice(),
                        episode_id.into_bytes().as_slice()
                    ],
                )
                .map_err(|error| {
                    StorageError::sqlite("complete feed-discovery download effect", error)
                })?;
            complete_workflows(transaction, now_ms)?;
            Ok(changed == 1)
        })
    }

    pub fn admit_feed_discovery_notification(
        &self,
        occurrence_id: FeedDiscoveryOccurrenceId,
        episode_id: EpisodeId,
        now_ms: i64,
        deadline_at_ms: i64,
    ) -> Result<Option<FeedDiscoveryEffectRecord>, StorageError> {
        self.write(|transaction| {
            let attempt: Option<i64> = transaction
                .query_row(
                    "SELECT attempt FROM pod0_feed_discovery_effects
                     WHERE occurrence_id=?1 AND episode_id=?2 AND kind='notification'
                       AND stage IN ('pending','retry_scheduled')
                       AND COALESCE(not_before_ms,0)<=?3",
                    params![
                        occurrence_id.into_bytes().as_slice(),
                        episode_id.into_bytes().as_slice(),
                        now_ms
                    ],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|error| StorageError::sqlite("read notification effect attempt", error))?;
            let Some(attempt) = attempt else {
                return Ok(None);
            };
            let attempt = u8::try_from(attempt)
                .ok()
                .and_then(|value| value.checked_add(1))
                .filter(|value| *value <= FEED_DISCOVERY_NOTIFICATION_MAX_ATTEMPTS)
                .ok_or(StorageError::CorruptSchema {
                    detail: "feed-discovery notification attempt is malformed",
                })?;
            let request_id = pod0_application::feed_discovery_notification_request_id(
                occurrence_id,
                episode_id,
                attempt,
            );
            transaction
                .execute(
                    "UPDATE pod0_feed_discovery_effects
                     SET stage='requested',attempt=?1,request_id=?2,not_before_ms=NULL,
                         deadline_at_ms=?3,failure_code=NULL,updated_at_ms=?4
                     WHERE occurrence_id=?5 AND episode_id=?6 AND kind='notification'",
                    params![
                        i64::from(attempt),
                        request_id.into_bytes().as_slice(),
                        deadline_at_ms,
                        now_ms,
                        occurrence_id.into_bytes().as_slice(),
                        episode_id.into_bytes().as_slice()
                    ],
                )
                .map_err(|error| {
                    StorageError::sqlite("admit feed-discovery notification", error)
                })?;
            read_effect(
                transaction,
                occurrence_id,
                episode_id,
                FeedDiscoveryEffectKind::Notification,
            )
        })
    }

    pub fn finish_feed_discovery_notification(
        &self,
        request_id: HostRequestId,
        outcome: FeedDiscoveryNotificationOutcome,
        now_ms: i64,
    ) -> Result<Option<FeedDiscoveryEffectRecord>, StorageError> {
        self.write(|transaction| {
            let current = effect_for_request(transaction, request_id)?;
            let Some(current) = current else {
                return Ok(None);
            };
            if current.stage != FeedDiscoveryEffectStage::Requested {
                return Ok(Some(current));
            }
            let expired = now_ms >= current.expires_at_ms;
            let retryable = outcome == FeedDiscoveryNotificationOutcome::RetryableFailure
                && current.attempt < FEED_DISCOVERY_NOTIFICATION_MAX_ATTEMPTS
                && !expired;
            let (stage, not_before, failure) = match outcome {
                FeedDiscoveryNotificationOutcome::Delivered => ("succeeded", None, None),
                FeedDiscoveryNotificationOutcome::RetryableFailure if retryable => (
                    "retry_scheduled",
                    Some(now_ms.saturating_add(FEED_DISCOVERY_NOTIFICATION_RETRY_MILLISECONDS)),
                    Some("platform_failure"),
                ),
                FeedDiscoveryNotificationOutcome::PermissionDenied => {
                    ("obsolete", None, Some("permission_denied"))
                }
                FeedDiscoveryNotificationOutcome::Cancelled => {
                    ("obsolete", None, Some("cancelled"))
                }
                FeedDiscoveryNotificationOutcome::RetryableFailure
                | FeedDiscoveryNotificationOutcome::PermanentFailure => {
                    ("failed", None, Some("delivery_failed"))
                }
            };
            transaction
                .execute(
                    "UPDATE pod0_feed_discovery_effects
                     SET stage=?1,request_id=NULL,deadline_at_ms=NULL,not_before_ms=?2,
                         failure_code=?3,updated_at_ms=?4
                     WHERE occurrence_id=?5 AND episode_id=?6 AND kind='notification'
                       AND request_id=?7",
                    params![
                        stage,
                        not_before,
                        failure,
                        now_ms,
                        current.occurrence_id.into_bytes().as_slice(),
                        current.episode_id.into_bytes().as_slice(),
                        request_id.into_bytes().as_slice()
                    ],
                )
                .map_err(|error| {
                    StorageError::sqlite("finish feed-discovery notification", error)
                })?;
            complete_workflows(transaction, now_ms)?;
            read_effect(
                transaction,
                current.occurrence_id,
                current.episode_id,
                FeedDiscoveryEffectKind::Notification,
            )
        })
    }

    pub fn expire_feed_discovery_notifications(
        &self,
        now_ms: i64,
    ) -> Result<Vec<HostRequestId>, StorageError> {
        self.write(|transaction| {
            let request_ids = requested_notification_ids(transaction, now_ms)?;
            for request_id in &request_ids {
                let _ = finish_notification_timeout(transaction, *request_id, now_ms)?;
            }
            complete_workflows(transaction, now_ms)?;
            Ok(request_ids)
        })
    }
}

include!("feed_discovery_workflow_store_policy.rs");
include!("feed_discovery_workflow_store_codec.rs");
include!("feed_discovery_workflow_store_read.rs");
