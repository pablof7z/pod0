use pod0_domain::{AutoDownloadMode, AutoDownloadPolicy};

use crate::feed_discovery_store_test_support::*;
use crate::feed_discovery_workflow_store_tests::{
    refresh_five, subscribe_with_existing_episode,
};
use crate::listening_import_test_support::id;
use crate::{FeedDiscoveryEffectKind, LibraryStore};

#[test]
fn off_and_all_new_download_policies_apply_to_the_original_batch() {
    let (_off_fixture, off_store) = empty_authoritative_store();
    let off_podcast = podcast(&off_store);
    subscribe_with_existing_episode(&off_store, &off_podcast);
    off_store
        .update_subscription_preferences(
            id(220),
            &"b".repeat(64),
            off_podcast.podcast_id,
            Some(AutoDownloadPolicy {
                mode: AutoDownloadMode::Off,
                wifi_only: false,
            }),
            None,
            BASE_TIME + 1,
        )
        .unwrap();
    let _ = refresh_five(&off_store, &off_podcast, BASE_TIME + 2);
    off_store
        .plan_pending_feed_discoveries(BASE_TIME + 3, 10)
        .unwrap();
    assert!(
        off_store
            .pending_feed_discovery_effects(
                FeedDiscoveryEffectKind::Download,
                BASE_TIME + 3,
                10,
            )
            .unwrap()
            .is_empty()
    );

    let (_all_fixture, all_store) = empty_authoritative_store();
    let all_podcast = podcast(&all_store);
    subscribe_with_existing_episode(&all_store, &all_podcast);
    all_store
        .update_subscription_preferences(
            id(221),
            &"c".repeat(64),
            all_podcast.podcast_id,
            Some(AutoDownloadPolicy {
                mode: AutoDownloadMode::AllNew,
                wifi_only: true,
            }),
            None,
            BASE_TIME + 1,
        )
        .unwrap();
    let _ = refresh_five(&all_store, &all_podcast, BASE_TIME + 2);
    all_store
        .plan_pending_feed_discoveries(BASE_TIME + 3, 10)
        .unwrap();
    assert_eq!(
        all_store
            .pending_feed_discovery_effects(
                FeedDiscoveryEffectKind::Download,
                BASE_TIME + 3,
                10,
            )
            .unwrap()
            .len(),
        5
    );
}

#[test]
fn initial_and_expired_discoveries_never_create_notifications() {
    let (_initial_fixture, initial_store) = empty_authoritative_store();
    let initial_podcast = podcast(&initial_store);
    initial_store
        .apply_feed(
            id(230),
            &"d".repeat(64),
            initial_podcast.clone(),
            Vec::new(),
            true,
            false,
            None,
            None,
            BASE_TIME,
        )
        .unwrap();
    enable_notifications(&initial_store, initial_podcast.podcast_id, id(231));
    initial_store
        .apply_feed(
            id(232),
            &"e".repeat(64),
            initial_podcast.clone(),
            vec![episode(initial_podcast.podcast_id, 20, BASE_TIME + 1)],
            false,
            true,
            None,
            None,
            BASE_TIME + 2,
        )
        .unwrap();
    initial_store
        .plan_pending_feed_discoveries(BASE_TIME + 3, 10)
        .unwrap();
    assert!(notifications(&initial_store, i64::MAX).is_empty());

    let (_expired_fixture, expired_store) = empty_authoritative_store();
    let expired_podcast = podcast(&expired_store);
    subscribe_with_existing_episode(&expired_store, &expired_podcast);
    let _ = refresh_five(&expired_store, &expired_podcast, BASE_TIME + 2);
    expired_store
        .plan_pending_feed_discoveries(
            BASE_TIME + 2 + pod0_application::FEED_DISCOVERY_NOTIFICATION_TTL_MILLISECONDS,
            10,
        )
        .unwrap();
    assert!(notifications(&expired_store, i64::MAX).is_empty());
}

#[test]
fn disabling_per_show_notifications_obsoletes_pending_delivery() {
    let (_fixture, store) = empty_authoritative_store();
    let podcast = podcast(&store);
    subscribe_with_existing_episode(&store, &podcast);
    let _ = refresh_five(&store, &podcast, BASE_TIME + 2);
    store
        .plan_pending_feed_discoveries(BASE_TIME + 3, 10)
        .unwrap();
    assert_eq!(notifications(&store, i64::MAX).len(), 3);

    store
        .update_subscription_preferences(
            id(240),
            &"f".repeat(64),
            podcast.podcast_id,
            None,
            Some(false),
            BASE_TIME + 4,
        )
        .unwrap();
    store
        .reconcile_feed_discovery_preferences(BASE_TIME + 4)
        .unwrap();
    assert!(notifications(&store, i64::MAX).is_empty());
}

fn enable_notifications(
    store: &LibraryStore,
    podcast_id: pod0_domain::PodcastId,
    command_id: pod0_domain::CommandId,
) {
    store
        .update_subscription_preferences(
            command_id,
            &"1".repeat(64),
            podcast_id,
            Some(AutoDownloadPolicy {
                mode: AutoDownloadMode::Off,
                wifi_only: false,
            }),
            Some(true),
            BASE_TIME + 1,
        )
        .unwrap();
}

fn notifications(store: &LibraryStore, now_ms: i64) -> Vec<crate::FeedDiscoveryEffectRecord> {
    store
        .pending_feed_discovery_effects(FeedDiscoveryEffectKind::Notification, now_ms, 10)
        .unwrap()
}
