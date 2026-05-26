//! Tests for [`super::inbox_handler`] — inbox build, dismiss, triage, and score coverage.
//!
//! Extracted from `inbox_handler.rs` to keep that file under the 500-line hard limit.

use super::*;
use chrono::TimeZone;
use podcast_core::{Episode, Podcast};

fn fixture_store(now_unix: i64) -> Arc<Mutex<PodcastStore>> {
    let mut store = PodcastStore::new();
    let podcast = Podcast::new("Heuristic Show");
    let podcast_id = podcast.id;

    // Three episodes: 1 hour old, 5 days old, 60 days old.
    let one_hour = Episode::new(
        podcast_id,
        "https://example.com/feed.xml",
        "guid-1h",
        "Fresh",
        url::Url::parse("https://ex.com/1.mp3").unwrap(),
        Utc.timestamp_opt(now_unix - 3_600, 0).unwrap(),
    );
    let five_days = Episode::new(
        podcast_id,
        "https://example.com/feed.xml",
        "guid-5d",
        "Mid",
        url::Url::parse("https://ex.com/2.mp3").unwrap(),
        Utc.timestamp_opt(now_unix - 5 * 24 * 3_600, 0).unwrap(),
    );
    let sixty_days = Episode::new(
        podcast_id,
        "https://example.com/feed.xml",
        "guid-60d",
        "Old",
        url::Url::parse("https://ex.com/3.mp3").unwrap(),
        Utc.timestamp_opt(now_unix - 60 * 24 * 3_600, 0).unwrap(),
    );
    store.subscribe(podcast, vec![one_hour, five_days, sixty_days]);
    Arc::new(Mutex::new(store))
}

fn empty_triage_cache() -> Arc<Mutex<HashMap<String, TriageResult>>> {
    Arc::new(Mutex::new(HashMap::new()))
}

#[test]
fn build_inbox_returns_unlistened_episodes_sorted_by_score() {
    let now = Utc::now().timestamp();
    let store = fixture_store(now);
    let dismissed = Arc::new(Mutex::new(HashSet::<String>::new()));
    let cache = empty_triage_cache();

    let items = build_inbox(&store, &dismissed, &cache);
    assert_eq!(items.len(), 3);

    // Just-published first, long-tail last.
    assert_eq!(items[0].episode_title, "Fresh");
    assert_eq!(items[2].episode_title, "Old");
    assert!(items[0].priority_score >= items[1].priority_score);
    assert!(items[1].priority_score >= items[2].priority_score);
}

#[test]
fn build_inbox_skips_dismissed_episodes() {
    let now = Utc::now().timestamp();
    let store = fixture_store(now);
    let dismissed = Arc::new(Mutex::new(HashSet::<String>::new()));
    let cache = empty_triage_cache();

    // Dismiss the freshest episode.
    let fresh_id = {
        let s = store.lock().unwrap();
        let (_, eps) = s.all_podcasts()[0];
        eps.iter().find(|e| e.title == "Fresh").unwrap().id.0.to_string()
    };
    dismissed.lock().unwrap().insert(fresh_id);

    let items = build_inbox(&store, &dismissed, &cache);
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|i| i.episode_title != "Fresh"));
}

#[test]
fn build_inbox_skips_played_episodes() {
    let now = Utc::now().timestamp();
    let store = fixture_store(now);
    let dismissed = Arc::new(Mutex::new(HashSet::<String>::new()));
    let cache = empty_triage_cache();

    // Mark "Fresh" as played in the store.
    let fresh_id = {
        let s = store.lock().unwrap();
        let (_, eps) = s.all_podcasts()[0];
        eps.iter().find(|e| e.title == "Fresh").unwrap().id.0.to_string()
    };
    store.lock().unwrap().mark_episode_played(&fresh_id);

    let items = build_inbox(&store, &dismissed, &cache);
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|i| i.episode_title != "Fresh"));
}

#[test]
fn build_inbox_uses_llm_score_when_cache_hit() {
    let now = Utc::now().timestamp();
    let store = fixture_store(now);
    let dismissed = Arc::new(Mutex::new(HashSet::<String>::new()));
    let cache = empty_triage_cache();

    // Grab the "Old" episode id (heuristic score ~0.15) and inject a high
    // LLM score for it. The score won't override "Fresh" (heuristic = 1.0)
    // positionally, but the LLM reason + categories should be visible on the
    // "Old" item wherever it lands.
    let old_id = {
        let s = store.lock().unwrap();
        let (_, eps) = s.all_podcasts()[0];
        eps.iter().find(|e| e.title == "Old").unwrap().id.0.to_string()
    };
    cache.lock().unwrap().insert(old_id.clone(), TriageResult {
        priority_score: 0.99,
        priority_reason: "Exceptional episode".to_owned(),
        categories: vec!["tech".to_owned()],
    });

    let items = build_inbox(&store, &dismissed, &cache);
    assert_eq!(items.len(), 3);

    // The "Old" item should carry LLM-sourced reason and categories.
    let old_item = items.iter().find(|i| i.episode_id == old_id)
        .expect("Old episode should be in inbox");
    assert_eq!(old_item.priority_reason.as_deref(), Some("Exceptional episode"));
    assert_eq!(old_item.ai_categories, vec!["tech"]);
    assert!((old_item.priority_score - 0.99).abs() < 0.001);

    // Heuristic-only items must not have ai_categories.
    let heuristic_items: Vec<_> = items.iter().filter(|i| i.episode_id != old_id).collect();
    assert!(heuristic_items.iter().all(|i| i.ai_categories.is_empty()));
}

#[test]
fn handle_dismiss_records_in_set_and_bumps_rev() {
    let now = Utc::now().timestamp();
    let store = fixture_store(now);
    let dismissed = Arc::new(Mutex::new(HashSet::<String>::new()));
    let rev = Arc::new(AtomicU64::new(0));
    let cache = empty_triage_cache();
    let runtime = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap(),
    );

    let result = handle_inbox_action(
        InboxAction::Dismiss { episode_id: "ep-7".into() },
        &store,
        &dismissed,
        &rev,
        &cache,
        &runtime,
    );
    assert_eq!(result["ok"], true);
    assert!(dismissed.lock().unwrap().contains("ep-7"));
    assert_eq!(rev.load(Ordering::Relaxed), 1);
}

#[test]
fn handle_triage_bumps_rev() {
    let now = Utc::now().timestamp();
    let store = fixture_store(now);
    let dismissed = Arc::new(Mutex::new(HashSet::<String>::new()));
    let rev = Arc::new(AtomicU64::new(0));
    let cache = empty_triage_cache();
    let runtime = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap(),
    );

    // Triage will attempt LLM calls that fail (Ollama likely not running in
    // unit test environment). The rev must still be bumped regardless of
    // whether LLM calls succeed.
    let result = handle_inbox_action(InboxAction::Triage, &store, &dismissed, &rev, &cache, &runtime);
    assert_eq!(result["ok"], true);
    assert_eq!(rev.load(Ordering::Relaxed), 1);
    // No dismissed entries added.
    assert!(dismissed.lock().unwrap().is_empty());
}

#[test]
fn handle_mark_listened_flips_store_flag() {
    let now = Utc::now().timestamp();
    let store = fixture_store(now);
    let dismissed = Arc::new(Mutex::new(HashSet::<String>::new()));
    let rev = Arc::new(AtomicU64::new(0));
    let cache = empty_triage_cache();
    let runtime = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap(),
    );

    let fresh_id = {
        let s = store.lock().unwrap();
        let (_, eps) = s.all_podcasts()[0];
        eps.iter().find(|e| e.title == "Fresh").unwrap().id.0.to_string()
    };

    let result = handle_inbox_action(
        InboxAction::MarkListened { episode_id: fresh_id.clone() },
        &store,
        &dismissed,
        &rev,
        &cache,
        &runtime,
    );
    assert_eq!(result["ok"], true);
    assert_eq!(rev.load(Ordering::Relaxed), 1);

    let played = store
        .lock()
        .unwrap()
        .all_podcasts()
        .iter()
        .flat_map(|(_, eps)| eps.iter())
        .find(|e| e.id.0.to_string() == fresh_id)
        .map(|e| e.played)
        .unwrap_or(false);
    assert!(played);
}

#[test]
fn score_buckets_match_documented_thresholds() {
    let now = 1_000_000_000;
    assert_eq!(score(now, now - 3_600).1, "Just published");
    assert_eq!(score(now, now - 2 * 24 * 3_600).1, "Recent");
    assert_eq!(score(now, now - 5 * 24 * 3_600).1, "This week");
    assert_eq!(score(now, now - 20 * 24 * 3_600).1, "From your library");
    assert_eq!(score(now, now - 100 * 24 * 3_600).1, "From your library");
}

#[test]
fn inbox_item_round_trips_with_all_fields() {
    let item = InboxItem {
        episode_id: "ep-42".into(),
        episode_title: "Pilot".into(),
        podcast_id: "pod-1".into(),
        podcast_title: "Some Show".into(),
        artwork_url: Some("https://ex.com/art.png".into()),
        published_at: 1_700_000_000,
        duration_secs: Some(2_700.0),
        priority_score: 0.87,
        priority_reason: Some("Just published".into()),
        ai_categories: vec![],
    };
    let json = serde_json::to_string(&item).expect("encode");
    let decoded: InboxItem = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, item);
}

#[test]
fn mark_episode_played_flips_flag_then_idempotent() {
    // Lives here (vs. `store::tests`) so it stays next to the only
    // production caller — `handle_inbox_action(MarkListened…)`.
    let now = Utc::now().timestamp();
    let store = fixture_store(now);
    let fresh_id = {
        let s = store.lock().unwrap();
        let (_, eps) = s.all_podcasts()[0];
        eps.iter().find(|e| e.title == "Fresh").unwrap().id.0.to_string()
    };
    let mut guard = store.lock().unwrap();
    assert!(guard.mark_episode_played(&fresh_id));
    assert!(!guard.mark_episode_played(&fresh_id));
    assert!(!guard.mark_episode_played("not-a-real-uuid"));
}

#[test]
fn inbox_item_omits_none_optionals() {
    let item = InboxItem {
        episode_id: "ep-1".into(),
        episode_title: "Pilot".into(),
        podcast_id: "pod-1".into(),
        podcast_title: "Some Show".into(),
        artwork_url: None,
        published_at: 1_700_000_000,
        duration_secs: None,
        priority_score: 0.5,
        priority_reason: None,
        ai_categories: vec![],
    };
    let json = serde_json::to_string(&item).expect("encode");
    assert!(!json.contains("artwork_url"));
    assert!(!json.contains("duration_secs"));
    assert!(!json.contains("priority_reason"));
    assert!(json.contains("\"priority_score\":0.5"));
}
