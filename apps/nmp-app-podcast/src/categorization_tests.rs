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
fn handle_run_categorizes_all_episodes() {
    let (store, _ep_id) = store_with_one(
        "AI is eating software",
        "A look at modern machine learning and the future of code.",
    );
    let cache: Arc<Mutex<HashMap<String, Vec<String>>>> = Arc::new(Mutex::new(HashMap::new()));
    let rev = Arc::new(AtomicU64::new(0));
    let runtime = Arc::new(tokio::runtime::Runtime::new().unwrap());
    // Pre-set the guard to `true` so the background LLM pass is NOT spawned:
    // this test isolates the synchronous phase-1 keyword pass and stays
    // hermetic (no Ollama, no task outliving the local runtime). The LLM
    // parse/filter path is covered by `categorization_llm_tests.rs`.
    let in_progress = Arc::new(AtomicBool::new(true));
    let result = handle_run(&store, &cache, &rev, &runtime, &in_progress);
    assert_eq!(result["ok"], true);
    // Phase-1 bumps rev exactly once; no background pass runs.
    assert_eq!(rev.load(Ordering::Relaxed), 1);
    let c = cache.lock().unwrap();
    assert_eq!(c.len(), 1);
    let labels = c.values().next().unwrap();
    assert!(labels.contains(&"Technology".to_owned()));
}
#[test]
fn handle_categorize_episode_writes_one_row() {
    let (store, ep_id) = store_with_one(
        "Quantum biology research",
        "A scientist on biology and physics in the lab.",
    );
    let cache: Arc<Mutex<HashMap<String, Vec<String>>>> = Arc::new(Mutex::new(HashMap::new()));
    let rev = AtomicU64::new(0);
    let result = handle_categorize_episode(&store, &cache, &rev, ep_id.clone());
    assert_eq!(result["ok"], true);
    assert!(result["categories"].is_array());
    let cats: Vec<String> = result["categories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    assert!(cats.contains(&"Science".to_owned()));
    assert_eq!(rev.load(Ordering::Relaxed), 1);
    let c = cache.lock().unwrap();
    assert!(c.contains_key(&ep_id));
}
#[test]
fn handle_categorize_episode_missing_episode_returns_error() {
    let (store, _ep_id) = store_with_one("Title", "Desc");
    let cache: Arc<Mutex<HashMap<String, Vec<String>>>> = Arc::new(Mutex::new(HashMap::new()));
    let rev = AtomicU64::new(0);
    let bogus = uuid::Uuid::new_v4().to_string();
    let result = handle_categorize_episode(&store, &cache, &rev, bogus);
    assert_eq!(result["ok"], false);
    assert!(result["error"].as_str().unwrap().contains("episode not found"));
    assert_eq!(rev.load(Ordering::Relaxed), 0);
}

