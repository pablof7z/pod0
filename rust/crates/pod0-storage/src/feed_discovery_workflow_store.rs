use pod0_application::{
    ActivitySubject, FEED_DISCOVERY_NOTIFICATION_MAX_ATTEMPTS,
    FEED_DISCOVERY_NOTIFICATION_RETRY_MILLISECONDS, FEED_DISCOVERY_NOTIFICATION_TTL_MILLISECONDS,
    LibraryFeedTransition, MAX_NEW_EPISODE_NOTIFICATIONS_PER_OCCURRENCE,
    NewEpisodeNotificationSettingsProjection,
};
use pod0_domain::{
    AutoDownloadMode, CancellationId, CommandId, EpisodeId, FeedDiscoveryOccurrenceId,
    HostRequestId, PodcastId, StateRevision,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use sha2::{Digest as _, Sha256};

use crate::StorageError;
use crate::feed_discovery_workflow_model::{
    FeedDiscoveryEffectKind, FeedDiscoveryEffectRecord, FeedDiscoveryEffectStage,
    FeedDiscoveryNotificationOutcome,
};
use crate::library_store::LibraryStore;
use crate::listening_db_codec::decode_auto_download;

impl LibraryStore {
    pub fn plan_pending_feed_discoveries(
        &self,
        now_ms: i64,
        maximum_count: u16,
    ) -> Result<usize, StorageError> {
        let occurrences = self.read(|connection| {
            pending_occurrences(connection, i64::from(maximum_count.clamp(1, 64)))
        })?;
        let mut planned = 0;
        for (occurrence_id, podcast_id, is_initial, observed_at_ms) in occurrences {
            planned += usize::from(self.commit_feed_discovery_recovery(
                b"plan",
                occurrence_id,
                ActivitySubject::Podcast { podcast_id },
                None,
                now_ms,
                |_| Ok(false),
                |transaction| Ok(workflow_for_occurrence(transaction, occurrence_id)?.is_none()),
                |transaction| {
                    plan_occurrence(
                        transaction,
                        occurrence_id,
                        podcast_id,
                        is_initial,
                        observed_at_ms,
                        now_ms,
                    )
                },
            )?);
        }
        Ok(planned)
    }

    pub fn reconcile_feed_discovery_preferences(&self, now_ms: i64) -> Result<usize, StorageError> {
        let occurrence_id = FeedDiscoveryOccurrenceId::from_bytes([0; 16]);
        self.commit_feed_discovery_recovery(
            b"preferences",
            occurrence_id,
            ActivitySubject::Global,
            None,
            now_ms,
            |transaction| has_obsolete_notification_effects(transaction, now_ms),
            |transaction| has_obsolete_notification_effects(transaction, now_ms),
            |transaction| {
                transaction
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
                complete_workflows(transaction, now_ms)
            },
        )
        .map(usize::from)
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
        self.commit_feed_discovery_recovery(
            b"download-applied",
            occurrence_id,
            ActivitySubject::Episode { episode_id },
            Some(episode_id),
            now_ms,
            |_| Ok(false),
            |transaction| {
                Ok(read_effect(
                    transaction,
                    occurrence_id,
                    episode_id,
                    FeedDiscoveryEffectKind::Download,
                )?
                .is_some_and(|record| record.stage == FeedDiscoveryEffectStage::Pending))
            },
            |transaction| {
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
                if changed != 1 {
                    return Err(StorageError::RevisionConflict);
                }
                complete_workflows(transaction, now_ms)
            },
        )
    }

    pub fn admit_feed_discovery_notification(
        &self,
        occurrence_id: FeedDiscoveryOccurrenceId,
        episode_id: EpisodeId,
        now_ms: i64,
        deadline_at_ms: i64,
    ) -> Result<Option<FeedDiscoveryEffectRecord>, StorageError> {
        self.commit_feed_notification_admission(occurrence_id, episode_id, now_ms, deadline_at_ms)
    }

    #[cfg(test)]
    pub(crate) fn finish_feed_discovery_notification(
        &self,
        request_id: HostRequestId,
        outcome: FeedDiscoveryNotificationOutcome,
        now_ms: i64,
    ) -> Result<Option<FeedDiscoveryEffectRecord>, StorageError> {
        self.write(|transaction| {
            finish_notification_in_transaction(transaction, request_id, outcome, now_ms)
        })
    }
}

fn has_obsolete_notification_effects(
    connection: &Connection,
    now_ms: i64,
) -> Result<bool, StorageError> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pod0_feed_discovery_effects AS effect WHERE \
             effect.kind='notification' AND effect.stage IN('pending','requested','retry_scheduled') \
             AND ((SELECT enabled FROM pod0_new_episode_notification_settings WHERE singleton=1)=0 \
             OR COALESCE((SELECT subscription.notifications_enabled FROM \
             pod0_feed_discovery_occurrences occurrence LEFT JOIN pod0_subscriptions subscription \
             ON subscription.podcast_id=occurrence.podcast_id WHERE \
             occurrence.occurrence_id=effect.occurrence_id),0)=0 OR EXISTS(SELECT 1 FROM \
             pod0_feed_discovery_workflows workflow WHERE workflow.occurrence_id=effect.occurrence_id \
             AND workflow.expires_at_ms<=?1)))",
            [now_ms],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("inspect obsolete notifications", error))
}

pub(crate) fn finish_notification_in_transaction(
    transaction: &Transaction<'_>,
    request_id: HostRequestId,
    outcome: FeedDiscoveryNotificationOutcome,
    now_ms: i64,
) -> Result<Option<FeedDiscoveryEffectRecord>, StorageError> {
    let Some(current) = effect_for_request(transaction, request_id)? else {
        return Ok(None);
    };
    if current.stage != FeedDiscoveryEffectStage::Requested {
        return Ok(Some(current));
    }
    let retryable = outcome == FeedDiscoveryNotificationOutcome::RetryableFailure
        && current.attempt < FEED_DISCOVERY_NOTIFICATION_MAX_ATTEMPTS
        && now_ms < current.expires_at_ms;
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
        FeedDiscoveryNotificationOutcome::Cancelled => ("obsolete", None, Some("cancelled")),
        FeedDiscoveryNotificationOutcome::RetryableFailure
        | FeedDiscoveryNotificationOutcome::PermanentFailure => {
            ("failed", None, Some("delivery_failed"))
        }
    };
    transaction
        .execute(
            "UPDATE pod0_feed_discovery_effects SET stage=?1,request_id=NULL,deadline_at_ms=NULL,\
             not_before_ms=?2,failure_code=?3,updated_at_ms=?4 WHERE occurrence_id=?5 AND \
             episode_id=?6 AND kind='notification' AND request_id=?7",
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
        .map_err(|error| StorageError::sqlite("finish feed-discovery notification", error))?;
    complete_workflows(transaction, now_ms)?;
    read_effect(
        transaction,
        current.occurrence_id,
        current.episode_id,
        FeedDiscoveryEffectKind::Notification,
    )
}

include!("feed_discovery_workflow_store_policy.rs");
include!("feed_discovery_workflow_store_recovery.rs");
include!("feed_discovery_workflow_store_codec.rs");
include!("feed_discovery_workflow_store_read.rs");
