use super::*;
use chrono::Utc;
use podcast_core::Episode;
use url::Url;

fn store_with_one(title: &str, description: &str) -> (Arc<Mutex<PodcastStore>>, String) {
    let mut store = PodcastStore::new();
    let podcast = podcast_core::Podcast::new("Show");
    let mut episode = Episode::new(
        podcast.id,
        "https://example.com/feed.xml",
        "guid-1",
        title,
        Url::parse("https://example.com/audio.mp3").unwrap(),
        Utc::now(),
    );
    episode.description = description.into();
    let ep_id = episode.id.0.to_string();
    store.subscribe(podcast, vec![episode]);
    (Arc::new(Mutex::new(store)), ep_id)
}

#[test]
fn handle_summarize_episode_errors_on_unknown_id() {
    let (store, _ep_id) = store_with_one("Title", "Desc");
    let rev = Arc::new(AtomicU64::new(0));
    let runtime = Arc::new(tokio::runtime::Runtime::new().unwrap());
    let result = handle_summarize_episode(&store, &rev, &runtime, "not-a-real-id".into());
    assert_eq!(result["ok"], false);
    // No background task spawned; rev untouched.
    assert_eq!(rev.load(Ordering::Relaxed), 0);
}

// NOTE: there is intentionally no "known id spawns + completes" test here.
// `handle_summarize_episode` spawns a real background Ollama call for a known
// episode; asserting its success synchronously would fire a live network
// request during the suite (the dev's Ollama is reachable), which is exactly
// the non-hermeticity the categorization author avoided with an in-progress
// guard. The synchronous success envelope is trivial (`{"ok":true,
// "status":"summarizing"}`); the LLM prompt/clean path is covered hermetically
// by `episode_summary_llm_tests.rs`, and the store stamp by
// `store::summary::tests`. The unknown-id path below is the meaningful
// branch — it returns before any spawn.
