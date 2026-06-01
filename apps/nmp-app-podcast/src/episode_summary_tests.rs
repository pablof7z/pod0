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

#[test]
fn handle_summarize_episode_accepts_known_id() {
    // Synchronous-path assertion only: the dispatch returns "summarizing"
    // immediately. The background Ollama call is exercised hermetically by
    // `episode_summary_llm_tests.rs` (prompt build + clean), not here — this
    // test must not depend on a live Ollama. The spawned task will fail to
    // connect and log without touching `rev` or the store.
    let (store, ep_id) = store_with_one("Title", "Desc");
    let rev = Arc::new(AtomicU64::new(0));
    let runtime = Arc::new(tokio::runtime::Runtime::new().unwrap());
    let result = handle_summarize_episode(&store, &rev, &runtime, ep_id);
    assert_eq!(result["ok"], true);
    assert_eq!(result["status"], "summarizing");
}
