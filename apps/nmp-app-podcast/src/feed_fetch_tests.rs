//! Tests for [`super::FeedFetchCoordinator`] — the async subscribe continuation.

use super::*;
use crate::state::inbox::InboxState;
use podcast_core::Podcast;
use podcast_feeds::http::{HttpReport, HttpResult};
use url::Url;

/// Minimal three-episode RSS, mirroring the headless mock feed. Enough for
/// `handle_feed_response` to parse a podcast title + three enclosures.
const FEED_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"
     xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd">
  <channel>
    <title>Mock Podcast</title>
    <link>http://127.0.0.1</link>
    <description>Test feed</description>
    <item>
      <title>Episode One</title>
      <enclosure url="http://127.0.0.1/ep1.mp3" length="1000000" type="audio/mpeg"/>
      <itunes:duration>1800</itunes:duration>
    </item>
    <item>
      <title>Episode Two</title>
      <enclosure url="http://127.0.0.1/ep2.mp3" length="2000000" type="audio/mpeg"/>
      <itunes:duration>2400</itunes:duration>
    </item>
    <item>
      <title>Episode Three</title>
      <enclosure url="http://127.0.0.1/ep3.mp3" length="1500000" type="audio/mpeg"/>
      <itunes:duration>3000</itunes:duration>
    </item>
  </channel>
</rss>"#;

fn coordinator_over(store: Arc<Mutex<PodcastStore>>) -> FeedFetchCoordinator {
    use crate::state::categories::CategoriesState;
    use crate::state::picks::PicksState;
    use crate::state::Infra;
    let infra = Infra::for_test();
    FeedFetchCoordinator::new(
        store.clone(),
        infra.rev.clone(),
        None, // no snapshot signal: skip the spawned categorize / picks / triage passes
        Arc::new(CategoriesState::for_test(store.clone())),
        Arc::new(PicksState::for_test(store.clone())),
        Arc::new(InboxState::for_test()),
    )
}

/// Build a coordinator that exposes the `InboxState` Arc for post-run
/// assertions.  Uses a real `SnapshotUpdateSignal` so the
/// `if self.snapshot_signal.is_some()` gate opens and `maybe_enqueue_triage`
/// is actually reached.
///
/// The `InboxState` is wired to the SAME `store` as the coordinator so
/// `maybe_enqueue_triage` sees the episodes hydrated by `apply_subscribe_result`.
fn coordinator_with_inbox(
    store: Arc<Mutex<PodcastStore>>,
) -> (FeedFetchCoordinator, Arc<InboxState>) {
    use crate::snapshot_signal::SnapshotUpdateSignal;
    use crate::state::categories::CategoriesState;
    use crate::state::picks::PicksState;
    use crate::state::Infra;
    let infra = Infra::for_test();
    // Create a throw-away channel — the receiver is dropped immediately so the
    // `send` inside `SnapshotUpdateSignal::bump` returns `Err(Disconnected)`,
    // which `bump` already ignores (`let _ = self.actor_tx.send(...)`).
    // The important thing is that `snapshot_signal.is_some()` is `true` so the
    // `maybe_enqueue_triage` gate opens — this matches the production path.
    // CommandSender wraps Sender<ActorMail>; create a throw-away channel.
    // The receiver is dropped immediately so send() returns Err(Disconnected),
    // which bump() already ignores (`let _ = self.actor_tx.send(...)`).
    let (tx, _rx) = std::sync::mpsc::channel::<nmp_core::ActorMail>();
    let signal = SnapshotUpdateSignal::new(infra.rev.clone(), nmp_core::CommandSender::new(tx));
    // Wire InboxState to the shared store so maybe_enqueue_triage sees the
    // episodes applied by apply_subscribe_result.
    let inbox = Arc::new(InboxState::new(infra.clone(), store.clone()));
    let coord = FeedFetchCoordinator::new(
        store.clone(),
        infra.rev.clone(),
        Some(signal),
        Arc::new(CategoriesState::for_test(store.clone())),
        Arc::new(PicksState::for_test(store.clone())),
        Arc::clone(&inbox),
    );
    (coord, inbox)
}

fn ok_result(body: &str) -> HttpResult {
    HttpResult::Ok {
        status_code: 200,
        headers: vec![],
        body: body.to_string(),
        body_base64: None,
    }
}

fn not_modified_result() -> HttpResult {
    HttpResult::Ok {
        status_code: 304,
        headers: vec![
            vec!["ETag".into(), "\"fresh-etag\"".into()],
            vec![
                "Last-Modified".into(),
                "Tue, 02 Jan 2024 00:00:00 GMT".into(),
            ],
        ],
        body: String::new(),
        body_base64: None,
    }
}

/// A report for an unregistered / already-resolved request id is a silent
/// no-op (D6) — never panics, mutates nothing.
#[test]
fn unknown_request_id_is_noop() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let coord = coordinator_over(store.clone());
    coord.apply_report(HttpReport {
        request_id: "missing".into(),
        result: ok_result(FEED_XML),
    });
    assert!(store.lock().unwrap().all_podcasts().is_empty());
}

/// The subscribe continuation parses the feed, replaces the optimistic
/// placeholder metadata, hydrates episodes, and keeps the follow membership.
#[test]
fn subscribe_report_hydrates_episodes() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let url = Url::parse("https://example.com/feed.xml").unwrap();
    let podcast_id = PodcastId::generate();

    // Optimistic placeholder, exactly as `handle_subscribe` inserts it.
    {
        let mut s = store.lock().unwrap();
        let mut placeholder = Podcast::new("example.com");
        placeholder.id = podcast_id;
        placeholder.feed_url = Some(url.clone());
        placeholder.title_is_placeholder = true;
        s.subscribe(placeholder, Vec::new());
        assert!(s.is_subscribed(podcast_id));
        assert_eq!(s.episodes_for(podcast_id).len(), 0);
    }

    let coord = coordinator_over(store.clone());
    let request_id = "req-1".to_string();
    coord.register(
        request_id.clone(),
        PendingFeedFetch {
            mode: FeedFetchMode::Subscribe,
            podcast_id,
            url,
            known: false,
        },
    );
    coord.apply_report(HttpReport {
        request_id: request_id.clone(),
        result: ok_result(FEED_XML),
    });

    let s = store.lock().unwrap();
    assert_eq!(
        s.episodes_for(podcast_id).len(),
        3,
        "episodes should hydrate from the parsed feed"
    );
    assert!(
        s.is_subscribed(podcast_id),
        "still followed after hydration"
    );
    assert_eq!(
        s.podcast(podcast_id).unwrap().title,
        "Mock Podcast",
        "placeholder title replaced by parsed feed title"
    );
}

/// A second report for the same request id (e.g. a duplicate platform callback)
/// is dropped — the pending entry was consumed by the first.
#[test]
fn duplicate_report_is_dropped() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let url = Url::parse("https://example.com/feed.xml").unwrap();
    let podcast_id = PodcastId::generate();
    {
        let mut s = store.lock().unwrap();
        let mut placeholder = Podcast::new("example.com");
        placeholder.id = podcast_id;
        placeholder.feed_url = Some(url.clone());
        s.subscribe(placeholder, Vec::new());
    }
    let coord = coordinator_over(store.clone());
    let request_id = "req-dup".to_string();
    coord.register(
        request_id.clone(),
        PendingFeedFetch {
            mode: FeedFetchMode::Subscribe,
            podcast_id,
            url,
            known: false,
        },
    );
    coord.apply_report(HttpReport {
        request_id: request_id.clone(),
        result: ok_result(FEED_XML),
    });
    // Second delivery: no pending entry remains, so it's a no-op.
    coord.apply_report(HttpReport {
        request_id,
        result: ok_result(FEED_XML),
    });
    assert_eq!(store.lock().unwrap().episodes_for(podcast_id).len(), 3);
}

/// A known-feed re-subscribe can legitimately receive 304: the optimistic
/// follow flip already surfaced the row, but the transport continuation must
/// still persist refreshed validators for the next conditional GET.
#[test]
fn known_subscribe_not_modified_updates_refresh_metadata_without_bump() {
    use std::sync::atomic::Ordering;

    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let url = Url::parse("https://example.com/feed.xml").unwrap();
    let podcast_id = PodcastId::generate();
    {
        let mut podcast = Podcast::new("Known Podcast");
        podcast.id = podcast_id;
        podcast.feed_url = Some(url.clone());
        podcast.etag = Some("\"old-etag\"".into());
        podcast.last_modified = Some("Mon, 01 Jan 2024 00:00:00 GMT".into());
        store.lock().unwrap().subscribe(podcast, Vec::new());
    }
    let coord = coordinator_over(store.clone());
    let rev_before = coord.rev.load(Ordering::Relaxed);
    let request_id = "req-known-304".to_string();
    coord.register(
        request_id.clone(),
        PendingFeedFetch {
            mode: FeedFetchMode::Subscribe,
            podcast_id,
            url,
            known: true,
        },
    );

    coord.apply_report(HttpReport {
        request_id,
        result: not_modified_result(),
    });

    assert_eq!(
        coord.rev.load(Ordering::Relaxed),
        rev_before,
        "304 metadata-only async subscribe result should not rebuild snapshots"
    );
    let store = store.lock().unwrap();
    let podcast = store.podcast(podcast_id).expect("podcast should remain");
    assert_eq!(podcast.etag.as_deref(), Some("\"fresh-etag\""));
    assert_eq!(
        podcast.last_modified.as_deref(),
        Some("Tue, 02 Jan 2024 00:00:00 GMT")
    );
    assert!(podcast.last_refreshed_at.is_some());
}

/// D8 trigger re-homing: an async subscribe report that delivers fresh
/// episodes MUST enqueue an inbox triage pass.
///
/// Asserts that `triage_in_progress` flips to `true` inside `maybe_enqueue_triage`
/// (the `InboxState` guard that prevents concurrent passes), mirroring the
/// pattern used to verify the categories/picks side-effects in their own
/// guard tests.
///
/// The coordinator is wired with a real `SnapshotUpdateSignal` so the
/// `if self.snapshot_signal.is_some()` gate opens — exactly the condition
/// that also enables `auto_categorize` and `auto_refresh_picks`.
#[test]
fn subscribe_report_with_fresh_episodes_enqueues_triage() {
    use std::sync::atomic::Ordering;
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let url = Url::parse("https://example.com/feed.xml").unwrap();
    let podcast_id = PodcastId::generate();

    // Insert optimistic placeholder (zero episodes) so the subscribe path has
    // a row to update — mirrors the production `handle_subscribe` flow.
    {
        let mut s = store.lock().unwrap();
        let mut placeholder = Podcast::new("example.com");
        placeholder.id = podcast_id;
        placeholder.feed_url = Some(url.clone());
        placeholder.title_is_placeholder = true;
        s.subscribe(placeholder, Vec::new());
    }

    let (coord, inbox) = coordinator_with_inbox(store.clone());

    // Precondition: triage not yet claimed.
    assert!(
        !inbox.triage_in_progress.load(Ordering::Relaxed),
        "triage_in_progress should start false"
    );

    let request_id = "req-triage".to_string();
    coord.register(
        request_id.clone(),
        PendingFeedFetch {
            mode: FeedFetchMode::Subscribe,
            podcast_id,
            url,
            known: false,
        },
    );
    coord.apply_report(HttpReport {
        request_id,
        result: ok_result(FEED_XML),
    });

    // Episodes hydrated — triage path was reached.
    assert_eq!(
        store.lock().unwrap().episodes_for(podcast_id).len(),
        3,
        "episodes must hydrate before the triage assertion"
    );

    // maybe_enqueue_triage claims the in-progress flag when it finds
    // untriaged episodes and spawns the background task.  The store now
    // holds three fresh (unscored) episodes, so the flag must have flipped.
    assert!(
        inbox.triage_in_progress.load(Ordering::Relaxed),
        "apply_subscribe_result must invoke maybe_enqueue_triage (D8 re-homing)"
    );
}
