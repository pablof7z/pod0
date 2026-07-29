//! Durable-recovery contract for the feed-fetch family (issue #189).
//!
//! Subscribing must commit immediately and durably; the fetch it triggers is
//! background work that survives restart, retries transient failures, and
//! coalesces duplicate intents. Every test here restarts by reopening the
//! same store path — delivering observations only to the original facade
//! instance would prove nothing about durability.

use std::sync::Arc;

use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

#[derive(Clone, Copy)]
struct FixedClock(i64);

impl pod0_application::Clock for FixedClock {
    fn now(&self) -> UnixTimestampMilliseconds {
        UnixTimestampMilliseconds::new(self.0)
    }
}

const SUBSCRIBE_FEED_URL: &str = "https://durable.example/subscribe-feed";
const RESTART_FEED_URL: &str = "https://durable.example/restart-feed";
const RETRY_FEED_URL: &str = "https://durable.example/retry-feed";
const COALESCE_FEED_URL: &str = "https://durable.example/coalesce-feed";

const DURABLE_FEED: &str = r#"
<rss version="2.0"><channel><title>Durable workflow fixture</title>
<item><title>Durable episode</title><guid>durable-workflow-episode</guid>
<pubDate>Mon, 20 Jul 2026 09:00:00 GMT</pubDate>
<enclosure url="https://durable.example/durable.mp3" type="audio/mpeg"/></item>
</channel></rss>"#;

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
    subscribe(&fixture.facade, 2, RESTART_FEED_URL);
    let issued = fixture.facade.next_host_requests(20);
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

    let reopened = Pod0Facade::open(target.to_string_lossy().into_owned()).unwrap();
    let offered = reopened.next_host_requests(20);
    let recovered = fetch_requests_for(&offered, RESTART_FEED_URL)
        .into_iter()
        .find(|candidate| candidate.request_id == original.request_id)
        .expect("restart must re-issue the in-flight feed fetch with the same request identity");

    let receipt = reopened.record_host_observation(feed_bytes_observation(&recovered));
    assert!(
        matches!(receipt, HostObservationReceipt::Persisted { .. }),
        "delivering the fetched bytes after restart must commit: {receipt:?}"
    );
    assert_eq!(durable_episode_count(&reopened), 1);
    assert!(subscribed_podcast_id(&reopened, RESTART_FEED_URL).is_some());

    let _ = reopened.record_host_observation(feed_bytes_observation(&recovered));
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
    let issued = fixture.facade.next_host_requests(20);
    let request = fetch_requests_for(&issued, RETRY_FEED_URL)
        .pop()
        .expect("subscribe should issue a feed fetch");

    fixture
        .facade
        .record_host_observation(HostObservationEnvelope {
            request_id: request.request_id,
            cancellation_id: request.cancellation_id,
            observed_request_revision: request.issued_revision,
            sequence_number: 0,
            observed_at: UnixTimestampMilliseconds::new(1_800_000_000_100),
            observation: HostObservation::Failed {
                code: HostFailureCode::Offline,
                safe_detail: None,
            },
        });
    assert_ne!(
        operation(&fixture.facade, command_id).stage,
        OperationStage::Failed,
        "an offline fetch failure must schedule a retry, not fail terminally"
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
    let offered = reopened.next_host_requests(20);
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

    let issued = fixture.facade.next_host_requests(20);
    let fetches = fetch_requests_for(&issued, COALESCE_FEED_URL);
    assert_eq!(
        fetches.len(),
        1,
        "duplicate subscribes for one feed must coalesce onto a single fetch"
    );

    let receipt = fixture
        .facade
        .record_host_observation(feed_bytes_observation(&fetches[0]));
    assert!(matches!(receipt, HostObservationReceipt::Persisted { .. }));
    assert_ne!(operation(&fixture.facade, first).stage, OperationStage::Failed);
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

fn subscribe(facade: &Pod0Facade, id: u64, feed_url: &str) -> CommandId {
    let command_id = CommandId::from_parts(88, id);
    facade.dispatch(CommandEnvelope {
        command_id,
        cancellation_id: CancellationId::from_parts(89, id),
        expected_revision: None,
        command: ApplicationCommand::SubscribeToFeed {
            feed_url: feed_url.to_owned(),
        },
    });
    command_id
}

fn library(facade: &Pod0Facade) -> LibraryProjection {
    let Projection::Library { value } = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::Library,
            offset: 0,
            max_items: 50,
        })
        .projection
    else {
        panic!("expected library projection");
    };
    value
}

fn operation(facade: &Pod0Facade, command_id: CommandId) -> OperationProjection {
    library(facade)
        .operations
        .into_iter()
        .find(|operation| operation.command_id == command_id)
        .expect("operation should be projected")
}

fn subscribed_podcast_id(facade: &Pod0Facade, feed_url: &str) -> Option<PodcastId> {
    let identity = pod0_application::normalize_feed_url(feed_url).unwrap();
    let value = library(facade);
    let podcast_id = value.podcasts.iter().find_map(|podcast| {
        podcast
            .feed_identity
            .as_ref()
            .filter(|feed| feed.comparison_key == identity.comparison_key)
            .map(|_| podcast.podcast_id)
    })?;
    value
        .subscriptions
        .iter()
        .find(|row| row.podcast_id == podcast_id)
        .map(|row| row.podcast_id)
}

fn fetch_requests_for(
    requests: &[HostRequestEnvelope],
    feed_url: &str,
) -> Vec<HostRequestEnvelope> {
    let identity = pod0_application::normalize_feed_url(feed_url).unwrap();
    requests
        .iter()
        .filter(|request| {
            matches!(
                &request.request,
                HostRequest::FetchFeed { feed_url, .. } if *feed_url == identity.source_url
            )
        })
        .cloned()
        .collect()
}

fn feed_bytes_observation(request: &HostRequestEnvelope) -> HostObservationEnvelope {
    let HostRequest::FetchFeed { feed_url, .. } = &request.request else {
        panic!("expected feed fetch request");
    };
    HostObservationEnvelope {
        request_id: request.request_id,
        cancellation_id: request.cancellation_id,
        observed_request_revision: request.issued_revision,
        sequence_number: 0,
        observed_at: UnixTimestampMilliseconds::new(1_800_000_100_000),
        observation: HostObservation::FeedBytesFetched {
            bytes: DURABLE_FEED.as_bytes().to_vec(),
            entity_tag: Some("\"durable-v1\"".to_owned()),
            last_modified: None,
            response_url: feed_url.clone(),
            http_status: 200,
        },
    }
}

fn durable_episode_count(facade: &Pod0Facade) -> usize {
    library(facade)
        .episodes
        .iter()
        .filter(|episode| episode.publisher_guid == "durable-workflow-episode")
        .count()
}
