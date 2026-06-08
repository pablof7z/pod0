//! Tests for [`super::FeedFetchCoordinator`] — the async subscribe continuation.

use super::*;
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
    FeedFetchCoordinator::new(
        store,
        Arc::new(AtomicU64::new(1)),
        None, // no snapshot signal: skip the spawned categorize / picks passes
        Arc::new(Mutex::new(HashMap::new())),
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(tokio::runtime::Runtime::new().unwrap()),
    )
}

fn ok_result(body: &str) -> HttpResult {
    HttpResult::Ok {
        status_code: 200,
        headers: vec![],
        body: body.to_string(),
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
    assert!(s.is_subscribed(podcast_id), "still followed after hydration");
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
