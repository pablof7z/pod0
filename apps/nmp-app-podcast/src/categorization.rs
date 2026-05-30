//! Actor-thread glue for episode categorization
//! ([`crate::ffi::actions::categorization_module`]).
//!
//! Wraps the pure `categorize_text` keyword matcher with the locking +
//! revision-bump dance that every `PodcastHostOpHandler` op needs, then
//! (M5.6) layers an LLM improvement pass on top:
//!
//! * `handle_run` — runs in two phases. **Phase 1 (synchronous):** scan
//!   every episode in [`PodcastStore`], compute its keyword label vector,
//!   and replace the kernel-side categorizer cache wholesale (this also
//!   drops labels for episodes removed from the library). Bumps `rev` so
//!   the UI gets fast initial tags. **Phase 2 (background spawn):** if no
//!   pass is already in flight, spawn an async task that re-categorizes
//!   each episode with the LLM ([`crate::categorization_llm`]) and
//!   re-stamps the cache per episode, bumping `rev` after each so the UI
//!   updates incrementally. Episodes where the LLM call fails keep their
//!   keyword tags. Returns `{"ok":true,"categorized":N}`.
//! * `handle_categorize_episode` — single-episode keyword refresh. Useful
//!   when the iOS shell wants to refresh one row without re-scanning the
//!   library. Stays on the keyword matcher (LLM scope is the `Run` path,
//!   which fires after feed refreshes). Returns
//!   `{"ok":true,"categories":["…"]}`.
//!
//! Locking discipline: `store` is read first into a small `Vec` of
//! `(episode_id, title, description)` triples, then the lock is dropped
//! before [`categorize_text`] runs the heuristic. The cache write
//! re-acquires its own lock. Snapshot reads observe a consistent
//! `HashMap` because `build_snapshot_payload` clones the whole map
//! under a short lock. The background LLM pass follows the same
//! lock-then-release discipline as `inbox_handler`'s triage task.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::runtime::Runtime;

use crate::categorization_llm::categorize_episode;
use crate::ffi::actions::categorization_module::categorize_text;
use crate::store::PodcastStore;

/// Re-run categorization over every episode in `store`.
///
/// **Phase 1 (synchronous keyword pass):** replaces the existing `cache`
/// contents wholesale with keyword-matched labels; episodes that have
/// since been removed from the library drop their labels. Bumps `rev` so
/// the UI gets fast initial tags.
///
/// **Phase 2 (background LLM pass):** unless a pass is already in flight
/// (tracked by `in_progress`), spawns an async task on `runtime` that
/// re-categorizes each episode with the LLM and re-stamps its cache entry,
/// bumping `rev` after each for incremental UI updates. Episodes where the
/// LLM call fails keep their keyword tags from phase 1.
///
/// Returns the number of episodes that picked up at least one keyword
/// category from phase 1 (the rest are stored as empty vecs so the
/// snapshot still skips them via `Vec::is_empty`).
pub(crate) fn handle_run(
    store: &Arc<Mutex<PodcastStore>>,
    cache: &Arc<Mutex<HashMap<String, Vec<String>>>>,
    rev: &Arc<AtomicU64>,
    runtime: &Arc<Runtime>,
    in_progress: &Arc<AtomicBool>,
) -> serde_json::Value {
    let snapshot: Vec<(String, String, String)> = match store.lock() {
        Ok(s) => s
            .all_podcasts()
            .into_iter()
            .flat_map(|(_podcast, episodes)| {
                episodes.iter().map(|ep| {
                    (
                        ep.id.0.to_string(),
                        ep.title.clone(),
                        ep.description.clone(),
                    )
                }).collect::<Vec<_>>()
            })
            .collect(),
        Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
    };

    let mut next: HashMap<String, Vec<String>> = HashMap::with_capacity(snapshot.len());
    let mut categorized: usize = 0;
    for (id, title, description) in snapshot {
        let cats = categorize_text(&title, &description);
        if !cats.is_empty() {
            categorized += 1;
        }
        next.insert(id, cats);
    }

    match cache.lock() {
        Ok(mut c) => {
            *c = next;
            rev.fetch_add(1, Ordering::Relaxed);
        }
        Err(_) => return serde_json::json!({"ok": false, "error": "categorizer cache poisoned"}),
    }

    // Phase 2: spawn the LLM improvement pass unless one is already running.
    // The guard gates only the spawn — the keyword pass above always runs.
    if in_progress
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_ok()
    {
        let store_c = Arc::clone(store);
        let cache_c = Arc::clone(cache);
        let rev_c = Arc::clone(rev);
        let runtime_c = Arc::clone(runtime);
        let in_progress_c = Arc::clone(in_progress);

        runtime.spawn(async move {
            categorize_in_background(store_c, cache_c, rev_c, runtime_c, in_progress_c).await;
        });
    }

    serde_json::json!({"ok": true, "categorized": categorized})
}

/// Background async categorization pass (M5.6). Runs off the actor thread so
/// the kernel is never blocked waiting for Ollama. Each successful result
/// re-stamps the episode's cache entry and bumps `rev` so the iOS snapshot
/// delivers incremental progress. Episodes where the LLM call fails keep the
/// keyword tags written by the synchronous phase-1 pass.
async fn categorize_in_background(
    store: Arc<Mutex<PodcastStore>>,
    cache: Arc<Mutex<HashMap<String, Vec<String>>>>,
    rev: Arc<AtomicU64>,
    runtime: Arc<Runtime>,
    in_progress: Arc<AtomicBool>,
) {
    // Collect episode metadata under a brief store lock then release it.
    let episodes: Vec<(String, String, String)> = {
        let guard = match store.lock() {
            Ok(g) => g,
            Err(_) => {
                in_progress.store(false, Ordering::Relaxed);
                return;
            }
        };
        guard
            .all_podcasts()
            .into_iter()
            .flat_map(|(_podcast, eps)| {
                eps.into_iter()
                    .map(|e| {
                        let ep_id = e.id.0.to_string();
                        let ep_title = e.title.clone();
                        let description: String = e.description.chars().take(500).collect();
                        (ep_id, ep_title, description)
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    };

    for (ep_id, ep_title, description) in episodes {
        let runtime2 = Arc::clone(&runtime);
        let result = tokio::task::spawn_blocking(move || {
            categorize_episode(&ep_title, &description, &runtime2)
        })
        .await;

        match result {
            Ok(Ok(cats)) => {
                if let Ok(mut c) = cache.lock() {
                    c.insert(ep_id, cats);
                }
                rev.fetch_add(1, Ordering::Relaxed);
            }
            Ok(Err(e)) => {
                eprintln!("[categorization] LLM categorize failed for {ep_id}: {e}");
            }
            Err(e) => {
                eprintln!("[categorization] spawn_blocking panicked for {ep_id}: {e}");
            }
        }
    }

    in_progress.store(false, Ordering::Relaxed);
    rev.fetch_add(1, Ordering::Relaxed);
}

/// Categorize a single episode, identified by its hyphenated UUID string.
/// Writes the labels into `cache` and bumps `rev`. Returns the labels
/// in a `categories` array.
pub(crate) fn handle_categorize_episode(
    store: &Arc<Mutex<PodcastStore>>,
    cache: &Arc<Mutex<HashMap<String, Vec<String>>>>,
    rev: &AtomicU64,
    episode_id: String,
) -> serde_json::Value {
    let (title, description) = match store.lock() {
        Ok(s) => match find_episode_text(&s, &episode_id) {
            Some(t) => t,
            None => {
                return serde_json::json!({
                    "ok": false,
                    "error": format!("episode not found: {episode_id}")
                })
            }
        },
        Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
    };

    let cats = categorize_text(&title, &description);

    match cache.lock() {
        Ok(mut c) => {
            c.insert(episode_id, cats.clone());
            rev.fetch_add(1, Ordering::Relaxed);
            serde_json::json!({"ok": true, "categories": cats})
        }
        Err(_) => serde_json::json!({"ok": false, "error": "categorizer cache poisoned"}),
    }
}

/// Look up `(title, description)` for an episode by id-string.
/// Returns `None` if the id doesn't match any episode in any subscribed
/// podcast.
fn find_episode_text(store: &PodcastStore, episode_id: &str) -> Option<(String, String)> {
    for (_podcast, episodes) in store.all_podcasts() {
        for ep in episodes {
            if ep.id.0.to_string() == episode_id {
                return Some((ep.title.clone(), ep.description.clone()));
            }
        }
    }
    None
}

#[cfg(test)]
#[path = "categorization_tests.rs"]
mod tests;
