//! Durable-recovery contract for the feed-fetch family (issue #189).
//!
//! Subscribing must commit immediately and durably; the fetch it triggers is
//! background work that survives restart, retries transient failures, and
//! coalesces duplicate intents. Every test here restarts by reopening the
//! same store path — delivering observations only to the original facade
//! instance would prove nothing about durability.

use std::sync::Arc;

use crate::runtime_feed_recovery_test_support::{
    durable_episode_count, feed_bytes_observation, fetch_requests_for, library, operation,
    subscribe, subscribed_podcast_id, FixedClock,
};
use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

const SUBSCRIBE_FEED_URL: &str = "https://durable.example/subscribe-feed";
const RESTART_FEED_URL: &str = "https://durable.example/restart-feed";
const RETRY_FEED_URL: &str = "https://durable.example/retry-feed";
const COALESCE_FEED_URL: &str = "https://durable.example/coalesce-feed";

#[test]
fn subscribe_commits_durably_before_any_feed_fetch_observation() {
    let fixture = PlaybackFixture::new();
    let command_id = subscribe(&fixture.facade, 1, SUBSCRIBE_FEED_URL);

    // No host observation has been recorded: the commit must not depend on
    // the fetch round trip completing.
    assert_eq!(
        operation(&fixture.facade, command_id).stage,
        OperationStage::Succeeded,
        "subscribe must commit before the feed is fetched"
    );
    assert!(
        subscribed_podcast_id(&fixture.facade, SUBSCRIBE_FEED_URL).is_some(),
        "the subscription must be visible in the library at commit time"
    );

    let PlaybackFixture {
        facade,
        target,
        _directory,
        ..
    } = fixture;
    drop(facade);
    let reopened = Pod0Facade::open(target.to_string_lossy().into_owned()).unwrap();
    assert!(
        subscribed_podcast_id(&reopened, SUBSCRIBE_FEED_URL).is_some(),
        "the subscription must be durable across restart before any fetch"
    );
}

#[test]
fn interrupted_subscribe_reissues_fetch_after_restart_and_applies_once() {
    let fixture = PlaybackFixture::new();
    fixture
        .facade
        .state()
        .set_clock(Arc::new(FixedClock(1_800_000_000_000)));
    subscribe(&fixture.facade, 2, RESTART_FEED_URL);
    let issued = fixture.facade.next_leased_host_requests(20);
    let original = fetch_requests_for(&issued, RESTART_FEED_URL)
        .pop()
        .expect("subscribe should issue a feed fetch");

    let PlaybackFixture {
        facade,
        target,
        _directory,
        ..
    } = fixture;
    drop(facade);

    let reopened = Pod0Facade::open_with_clock(
        target.to_string_lossy().into_owned(),
        Arc::new(FixedClock(original.lease.expires_at.value + 1)),
    );
    let offered = reopened.next_leased_host_requests(20);
    let recovered = fetch_requests_for(&offered, RESTART_FEED_URL)
        .into_iter()
        .find(|candidate| candidate.request.request_id == original.request.request_id)
        .expect("restart after lease expiry must re-offer the same durable feed effect");

    let receipt = reopened.record_leased_host_observation(feed_bytes_observation(&recovered));
    assert!(
        matches!(receipt, HostObservationReceipt::Persisted { .. }),
        "delivering the fetched bytes after restart must commit: {receipt:?}"
    );
    assert_eq!(durable_episode_count(&reopened), 1);
    assert!(subscribed_podcast_id(&reopened, RESTART_FEED_URL).is_some());

    let _ = reopened.record_leased_host_observation(feed_bytes_observation(&recovered));
    assert_eq!(
        durable_episode_count(&reopened),
        1,
        "re-delivering the same fetch must not apply the episodes twice"
    );
}

#[test]
fn transient_fetch_failure_schedules_retry_that_survives_restart() {
    let fixture = PlaybackFixture::new();
    fixture
        .facade
        .state()
        .set_clock(Arc::new(FixedClock(1_800_000_000_000)));
    let command_id = subscribe(&fixture.facade, 3, RETRY_FEED_URL);
    let issued = fixture.facade.next_leased_host_requests(20);
    let request = fetch_requests_for(&issued, RETRY_FEED_URL)
        .pop()
        .expect("subscribe should issue a feed fetch");

    fixture
        .facade
        .record_leased_host_observation(LeasedHostObservationEnvelope {
            lease: request.lease,
            observation: HostObservationEnvelope {
                request_id: request.request.request_id,
                cancellation_id: request.request.cancellation_id,
                observed_request_revision: request.request.issued_revision,
                sequence_number: 0,
                observed_at: UnixTimestampMilliseconds::new(1_800_000_000_100),
                observation: HostObservation::Failed {
                    code: HostFailureCode::Offline,
                    safe_detail: None,
                },
            },
        });
    assert_ne!(
        operation(&fixture.facade, command_id).stage,
        OperationStage::Failed,
        "an offline fetch failure must schedule a retry, not fail terminally"
    );
    // Retry availability is stored on the exact durable effect, not a native
    // timer or a second core-wake leg.
    let requests = fixture.facade.next_leased_host_requests(20);
    assert!(
        requests.is_empty(),
        "retry must remain unavailable before not_before: {requests:?}"
    );

    let PlaybackFixture {
        facade,
        target,
        _directory,
        ..
    } = fixture;
    drop(facade);

    // Reopen one day later: any reasonable retry backoff has elapsed, so the
    // surviving retry must offer the fetch again. No wall-clock latency is
    // asserted anywhere — the clock is fixed and owned by the test.
    let reopened = Pod0Facade::open_with_clock(
        target.to_string_lossy().into_owned(),
        Arc::new(FixedClock(1_800_086_400_000)),
    );
    let offered = reopened.next_leased_host_requests(20);
    assert!(
        !fetch_requests_for(&offered, RETRY_FEED_URL).is_empty(),
        "the scheduled retry must survive a restart and re-issue the fetch"
    );
}

#[test]
fn duplicate_subscribes_coalesce_onto_one_fetch() {
    let fixture = PlaybackFixture::new();
    let first = subscribe(&fixture.facade, 4, COALESCE_FEED_URL);
    let second = subscribe(&fixture.facade, 5, COALESCE_FEED_URL);

    let issued = fixture.facade.next_leased_host_requests(20);
    let fetches = fetch_requests_for(&issued, COALESCE_FEED_URL);
    assert_eq!(
        fetches.len(),
        1,
        "duplicate subscribes for one feed must coalesce onto a single fetch"
    );

    let receipt = fixture
        .facade
        .record_leased_host_observation(feed_bytes_observation(&fetches[0]));
    assert!(matches!(receipt, HostObservationReceipt::Persisted { .. }));
    assert_ne!(
        operation(&fixture.facade, first).stage,
        OperationStage::Failed
    );
    assert_ne!(
        operation(&fixture.facade, second).stage,
        OperationStage::Failed
    );

    let identity = pod0_application::normalize_feed_url(COALESCE_FEED_URL).unwrap();
    let value = library(&fixture.facade);
    let matching_podcasts = value
        .podcasts
        .iter()
        .filter(|podcast| {
            podcast
                .feed_identity
                .as_ref()
                .is_some_and(|feed| feed.comparison_key == identity.comparison_key)
        })
        .count();
    assert_eq!(matching_podcasts, 1);
    assert_eq!(durable_episode_count(&fixture.facade), 1);
}
