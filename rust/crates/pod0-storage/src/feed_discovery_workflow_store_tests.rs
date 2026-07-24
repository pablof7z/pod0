use pod0_domain::{AutoDownloadMode, AutoDownloadPolicy};

use crate::feed_discovery_store_test_support::*;
use crate::listening_import_test_support::id;
use crate::{
    FeedDiscoveryEffectKind, FeedDiscoveryEffectStage, FeedDiscoveryNotificationOutcome,
    LibraryStore, StorageError,
};

#[test]
fn original_batch_plans_latest_downloads_and_capped_notifications_once() {
    let (fixture, store) = empty_authoritative_store();
    let podcast = podcast(&store);
    subscribe_with_existing_episode(&store, &podcast);
    store
        .update_subscription_preferences(
            id(201),
            &"1".repeat(64),
            podcast.podcast_id,
            Some(AutoDownloadPolicy {
                mode: AutoDownloadMode::Latest { count: 2 },
                wifi_only: true,
            }),
            Some(true),
            BASE_TIME + 1,
        )
        .unwrap();
    let occurrence = refresh_five(&store, &podcast, BASE_TIME + 2);

    assert_eq!(
        store
            .plan_pending_feed_discoveries(BASE_TIME + 3, 10)
            .unwrap(),
        1
    );
    assert_eq!(
        store
            .plan_pending_feed_discoveries(BASE_TIME + 4, 10)
            .unwrap(),
        0
    );
    let downloads = store
        .pending_feed_discovery_effects(
            FeedDiscoveryEffectKind::Download,
            BASE_TIME + 4,
            10,
        )
        .unwrap();
    let notifications = store
        .pending_feed_discovery_effects(
            FeedDiscoveryEffectKind::Notification,
            BASE_TIME + 4,
            10,
        )
        .unwrap();
    assert_eq!(downloads.len(), 2);
    assert_eq!(notifications.len(), 3);
    assert_eq!(downloads[0].occurrence_id, occurrence);
    assert_eq!(downloads[0].episode_title, "Episode 14");
    assert_eq!(downloads[1].episode_title, "Episode 13");
    assert_eq!(notifications[2].episode_title, "Episode 12");
    assert!(downloads.iter().all(|effect| effect.command_id.is_some()));

    drop(store);
    let reopened = LibraryStore::open_authoritative(&fixture.target).unwrap();
    assert_eq!(
        reopened
            .pending_feed_discovery_effects(
                FeedDiscoveryEffectKind::Notification,
                BASE_TIME + 4,
                10,
            )
            .unwrap(),
        notifications
    );
}

#[test]
fn notification_retry_settings_and_terminal_outcomes_are_durable_and_idempotent() {
    let (_fixture, store) = empty_authoritative_store();
    let podcast = podcast(&store);
    subscribe_with_existing_episode(&store, &podcast);
    let _ = refresh_five(&store, &podcast, BASE_TIME + 2);
    store
        .plan_pending_feed_discoveries(BASE_TIME + 3, 10)
        .unwrap();
    let pending = store
        .pending_feed_discovery_effects(
            FeedDiscoveryEffectKind::Notification,
            BASE_TIME + 3,
            10,
        )
        .unwrap();
    let first = &pending[0];
    let requested = store
        .admit_feed_discovery_notification(
            first.occurrence_id,
            first.episode_id,
            BASE_TIME + 4,
            BASE_TIME + 100,
        )
        .unwrap()
        .unwrap();
    assert_eq!(requested.stage, FeedDiscoveryEffectStage::Requested);
    assert_eq!(requested.attempt, 1);
    let request_id = requested.request_id.unwrap();
    let retry = store
        .finish_feed_discovery_notification(
            request_id,
            FeedDiscoveryNotificationOutcome::RetryableFailure,
            BASE_TIME + 5,
        )
        .unwrap()
        .unwrap();
    assert_eq!(retry.stage, FeedDiscoveryEffectStage::RetryScheduled);
    assert!(retry.not_before_ms.unwrap() > BASE_TIME + 5);
    assert!(
        store
            .pending_feed_discovery_effects(
                FeedDiscoveryEffectKind::Notification,
                BASE_TIME + 6,
                10,
            )
            .unwrap()
            .iter()
            .all(|effect| effect.episode_id != first.episode_id)
    );

    let due_at = retry.not_before_ms.unwrap();
    let due = store
        .pending_feed_discovery_effects(
            FeedDiscoveryEffectKind::Notification,
            due_at,
            10,
        )
        .unwrap()
        .into_iter()
        .find(|effect| effect.episode_id == first.episode_id)
        .unwrap();
    let second = store
        .admit_feed_discovery_notification(
            due.occurrence_id,
            due.episode_id,
            due_at,
            due_at + 100,
        )
        .unwrap()
        .unwrap();
    assert_eq!(second.attempt, 2);
    assert_ne!(second.request_id, Some(request_id));
    let delivered = store
        .finish_feed_discovery_notification(
            second.request_id.unwrap(),
            FeedDiscoveryNotificationOutcome::Delivered,
            due_at + 1,
        )
        .unwrap()
        .unwrap();
    assert_eq!(delivered.stage, FeedDiscoveryEffectStage::Succeeded);

    let disabled = store
        .set_new_episode_notifications_enabled(
            id(202),
            &"2".repeat(64),
            false,
            due_at + 2,
        )
        .unwrap();
    assert!(!disabled.enabled);
    assert_eq!(
        store
            .set_new_episode_notifications_enabled(
                id(202),
                &"2".repeat(64),
                false,
                due_at + 3,
            )
            .unwrap(),
        disabled
    );
    assert_eq!(
        store.set_new_episode_notifications_enabled(
            id(202),
            &"3".repeat(64),
            true,
            due_at + 4,
        ),
        Err(StorageError::CommandConflict)
    );
    store
        .reconcile_feed_discovery_preferences(due_at + 2)
        .unwrap();
    assert!(
        store
            .pending_feed_discovery_effects(
                FeedDiscoveryEffectKind::Notification,
                i64::MAX,
                10,
            )
            .unwrap()
            .is_empty()
    );
}

pub(super) fn subscribe_with_existing_episode(
    store: &LibraryStore,
    podcast: &pod0_domain::PodcastRecord,
) {
    store
        .apply_feed(
            id(200),
            &"0".repeat(64),
            podcast.clone(),
            vec![episode(podcast.podcast_id, 1, BASE_TIME)],
            true,
            false,
            None,
            None,
            BASE_TIME,
        )
        .unwrap();
    store
        .update_subscription_preferences(
            id(199),
            &"9".repeat(64),
            podcast.podcast_id,
            None,
            Some(true),
            BASE_TIME + 1,
        )
        .unwrap();
}

pub(super) fn refresh_five(
    store: &LibraryStore,
    podcast: &pod0_domain::PodcastRecord,
    observed_at_ms: i64,
) -> pod0_domain::FeedDiscoveryOccurrenceId {
    store
        .apply_feed(
            id(210),
            &"a".repeat(64),
            podcast.clone(),
            (10..15)
                .map(|value| {
                    episode(
                        podcast.podcast_id,
                        value,
                        BASE_TIME + i64::try_from(value).unwrap(),
                    )
                })
                .collect(),
            false,
            true,
            None,
            None,
            observed_at_ms,
        )
        .unwrap()
        .discovery_occurrence_id
        .unwrap()
}
