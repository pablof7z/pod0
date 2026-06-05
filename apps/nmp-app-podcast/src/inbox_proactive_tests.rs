//! Tests for the proactive inbox-triage trigger in [`super::inbox_handler`].
//!
//! Covers status-aware `build_inbox` fallback, the pure
//! `episodes_needing_triage` predicate (no-entry / fresh-Ready / stale-Ready /
//! Pending-in-cooldown / Pending-past-cooldown), and `maybe_enqueue_triage`
//! dispatch (claims a pass when work is due, no-ops otherwise).
//!
//! Split out of `inbox_handler_tests.rs` to keep both files under the 500-line
//! hard cap (AGENTS.md).

use super::*;
use chrono::Utc;
use podcast_core::{Episode, Podcast};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Build the same three-episode fixture (1h / 5d / 60d old) used by
/// `inbox_handler_tests`, all unlistened.
fn fixture_store(now_unix: i64) -> Arc<Mutex<PodcastStore>> {
    use chrono::TimeZone;
    let mut store = PodcastStore::new();
    let podcast = Podcast::new("Heuristic Show");
    let podcast_id = podcast.id;

    let mk = |guid: &str, title: &str, age_secs: i64, n: &str| {
        Episode::new(
            podcast_id,
            "https://example.com/feed.xml",
            guid,
            title,
            url::Url::parse(&format!("https://ex.com/{n}.mp3")).unwrap(),
            Utc.timestamp_opt(now_unix - age_secs, 0).unwrap(),
        )
    };
    let one_hour = mk("guid-1h", "Fresh", 3_600, "1");
    let five_days = mk("guid-5d", "Mid", 5 * 24 * 3_600, "2");
    let sixty_days = mk("guid-60d", "Old", 60 * 24 * 3_600, "3");
    store.subscribe(podcast, vec![one_hour, five_days, sixty_days]);
    Arc::new(Mutex::new(store))
}

fn empty_triage_cache() -> Arc<Mutex<HashMap<String, TriageResult>>> {
    Arc::new(Mutex::new(HashMap::new()))
}

/// Collect the unlistened-episode ids from the fixture store, matching the set
/// `maybe_enqueue_triage` would walk.
fn unlistened_ids(store: &Arc<Mutex<PodcastStore>>) -> Vec<String> {
    let s = store.lock().unwrap();
    s.all_podcasts()
        .into_iter()
        .flat_map(|(_, eps)| {
            eps.iter()
                .filter(|e| !e.played)
                .map(|e| e.id.0.to_string())
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn build_inbox_ignores_pending_entry_and_uses_heuristic() {
    let now = Utc::now().timestamp();
    let store = fixture_store(now);
    let dismissed = Arc::new(Mutex::new(HashSet::<String>::new()));
    let cache = empty_triage_cache();

    // "Old" gets a Pending (failure-placeholder) entry — its inert score
    // fields must NOT be used; build_inbox must fall back to the heuristic.
    let old_id = {
        let s = store.lock().unwrap();
        let (_, eps) = s.all_podcasts()[0];
        eps.iter().find(|e| e.title == "Old").unwrap().id.0.to_string()
    };
    cache.lock().unwrap().insert(old_id.clone(), TriageResult::pending(now));

    let items = build_inbox(&store, &dismissed, &cache);
    let old_item = items.iter().find(|i| i.episode_id == old_id).unwrap();
    // Heuristic for a 60-day-old episode is the long-tail floor (0.15), with
    // its "From your library" reason — NOT the Pending placeholder's 0.0.
    assert!((old_item.priority_score - 0.15).abs() < 0.001);
    assert_eq!(old_item.priority_reason.as_deref(), Some("From your library"));
    assert!(old_item.ai_categories.is_empty());
}

#[test]
fn needs_triage_true_when_no_entry() {
    let now = 1_000_000_000;
    let cache: HashMap<String, TriageResult> = HashMap::new();
    assert!(episodes_needing_triage(&cache, &["ep-1".to_owned()], now));
}

#[test]
fn needs_triage_false_for_fresh_ready_entry() {
    let now = 1_000_000_000;
    let mut cache = HashMap::new();
    cache.insert(
        "ep-1".to_owned(),
        TriageResult::ready(0.8, "ok".into(), vec![], now - 60),
    );
    assert!(!episodes_needing_triage(&cache, &["ep-1".to_owned()], now));
}

#[test]
fn needs_triage_true_for_stale_ready_entry() {
    let now = 1_000_000_000;
    let mut cache = HashMap::new();
    // Ready but attempted just over 24h ago → stale → re-triage.
    cache.insert(
        "ep-1".to_owned(),
        TriageResult::ready(0.8, "ok".into(), vec![], now - TRIAGE_STALE_SECS - 1),
    );
    assert!(episodes_needing_triage(&cache, &["ep-1".to_owned()], now));
}

#[test]
fn needs_triage_false_for_pending_within_cooldown() {
    let now = 1_000_000_000;
    let mut cache = HashMap::new();
    // Pending attempted 1 minute ago → still in cooldown → do NOT retry
    // (this is the offline-Ollama hot-loop guard).
    cache.insert("ep-1".to_owned(), TriageResult::pending(now - 60));
    assert!(!episodes_needing_triage(&cache, &["ep-1".to_owned()], now));
}

#[test]
fn needs_triage_true_for_pending_past_cooldown() {
    let now = 1_000_000_000;
    let mut cache = HashMap::new();
    cache.insert(
        "ep-1".to_owned(),
        TriageResult::pending(now - TRIAGE_RETRY_COOLDOWN_SECS - 1),
    );
    assert!(episodes_needing_triage(&cache, &["ep-1".to_owned()], now));
}

#[test]
fn needs_triage_true_if_any_episode_needs_it() {
    let now = 1_000_000_000;
    let mut cache = HashMap::new();
    // ep-1 is fresh+Ready; ep-2 has no entry → the set still needs triage.
    cache.insert(
        "ep-1".to_owned(),
        TriageResult::ready(0.8, "ok".into(), vec![], now),
    );
    let ids = vec!["ep-1".to_owned(), "ep-2".to_owned()];
    assert!(episodes_needing_triage(&cache, &ids, now));
}

#[test]
fn maybe_enqueue_triage_sets_in_progress_when_cache_empty() {
    let now = Utc::now().timestamp();
    let store = fixture_store(now);
    let cache = empty_triage_cache();
    let rev = Arc::new(AtomicU64::new(0));
    let runtime = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap(),
    );
    let in_progress = Arc::new(AtomicBool::new(false));

    maybe_enqueue_triage(&store, &cache, &rev, &runtime, &in_progress);

    // Empty cache + unlistened episodes → a pass is claimed synchronously.
    assert!(in_progress.load(Ordering::Relaxed));
}

#[test]
fn maybe_enqueue_triage_noop_when_all_fresh_ready() {
    let now = Utc::now().timestamp();
    let store = fixture_store(now);
    let cache = empty_triage_cache();
    let rev = Arc::new(AtomicU64::new(0));
    let runtime = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap(),
    );
    let in_progress = Arc::new(AtomicBool::new(false));

    // Pre-fill a fresh Ready entry for every unlistened episode.
    {
        let mut c = cache.lock().unwrap();
        for id in unlistened_ids(&store) {
            c.insert(id, TriageResult::ready(0.9, "graded".into(), vec![], now));
        }
    }

    maybe_enqueue_triage(&store, &cache, &rev, &runtime, &in_progress);

    // Nothing needs triage → no pass claimed, rev untouched.
    assert!(!in_progress.load(Ordering::Relaxed));
    assert_eq!(rev.load(Ordering::Relaxed), 0);
}

#[test]
fn maybe_enqueue_triage_noop_when_already_in_progress() {
    let now = Utc::now().timestamp();
    let store = fixture_store(now);
    let cache = empty_triage_cache(); // empty → would normally enqueue
    let rev = Arc::new(AtomicU64::new(0));
    let runtime = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap(),
    );
    // A pass is already running.
    let in_progress = Arc::new(AtomicBool::new(true));

    maybe_enqueue_triage(&store, &cache, &rev, &runtime, &in_progress);

    // Flag stays true (we didn't clear it) and we didn't spawn / bump rev.
    assert!(in_progress.load(Ordering::Relaxed));
    assert_eq!(rev.load(Ordering::Relaxed), 0);
}

#[test]
fn stamp_pending_leaves_ready_entries_untouched() {
    // A re-triage pass over an existing Ready entry must leave a good score
    // entirely alone — score, reason, categories, AND its attempted_at clock —
    // so a freshly-graded episode is never downgraded or churned.
    let cache = empty_triage_cache();
    let old_attempt = 1_000;
    cache.lock().unwrap().insert(
        "ep-1".to_owned(),
        TriageResult::ready(0.9, "graded".into(), vec!["tech".into()], old_attempt),
    );

    stamp_pending(&cache, "ep-1".to_owned());

    let c = cache.lock().unwrap();
    let tr = c.get("ep-1").unwrap();
    assert_eq!(tr.status, TriageStatus::Ready, "Ready must not be downgraded");
    assert!((tr.priority_score - 0.9).abs() < 0.001);
    assert_eq!(tr.priority_reason, "graded");
    assert_eq!(tr.categories, vec!["tech".to_owned()]);
    assert_eq!(
        tr.attempted_at, old_attempt,
        "Ready entries are left untouched — attempted_at is NOT advanced (see reconcile_pending)"
    );
}

#[test]
fn stamp_pending_writes_placeholder_when_missing_or_pending() {
    // No entry → fresh Pending placeholder.
    let cache = empty_triage_cache();
    stamp_pending(&cache, "ep-1".to_owned());
    {
        let c = cache.lock().unwrap();
        assert_eq!(c.get("ep-1").unwrap().status, TriageStatus::Pending);
    }

    // Existing Pending → still Pending, attempted_at refreshed to now.
    cache
        .lock()
        .unwrap()
        .insert("ep-2".to_owned(), TriageResult::pending(1_000));
    stamp_pending(&cache, "ep-2".to_owned());
    let c = cache.lock().unwrap();
    let tr = c.get("ep-2").unwrap();
    assert_eq!(tr.status, TriageStatus::Pending);
    assert!(tr.attempted_at > 1_000, "cooldown clock must reset on retry-failure");
}
