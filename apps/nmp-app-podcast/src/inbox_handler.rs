//! Inbox triage (feature #31).
//!
//! Owns both the **projection** (turn the store + dismissed-set into the
//! `Vec<InboxItem>` that surfaces on `PodcastUpdate.inbox`) and the
//! **action handlers** (`triage` / `dismiss` / `mark_listened`).
//!
//! Lives in its own crate-root module rather than under `ffi/` because the
//! projection is consumed by `ffi::snapshot::build_snapshot_payload` and
//! the handlers are consumed by `host_op_handler::PodcastHostOpHandler`.
//! Keeping it sibling-level lets both call sites import without crossing
//! through the snapshot module's private surface.
//!
//! ## Scoring strategy
//!
//! `build_inbox` checks the LLM triage cache first. If a cache entry
//! exists for an episode, its `priority_score`, `priority_reason`, and
//! `categories` are used verbatim. If not, the recency-bucket heuristic
//! (`score()`) is the fallback so the inbox always renders something useful
//! even before the first `Triage` action fires or when Ollama is offline.
//!
//! `InboxAction::Triage` runs LLM classification for all unlistened
//! episodes and populates the cache. This blocks the actor thread for the
//! duration of the LLM calls; see `inbox_llm` module docs for the
//! known-tradeoff note.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use tokio::runtime::Runtime;

use crate::ffi::actions::inbox_module::InboxAction;
use crate::ffi::projections::InboxItem;
use crate::inbox_llm::{triage_episode, TriageResult};
use crate::store::PodcastStore;

/// Build the `Vec<InboxItem>` for one snapshot tick.
///
/// Walks every subscribed podcast, picks the unlistened-and-not-dismissed
/// episodes, and scores them. If a `TriageResult` exists in `triage_cache`
/// for an episode, LLM-derived values are used; otherwise the recency
/// heuristic provides the fallback.
///
/// Reads `store`, `dismissed`, and `triage_cache` under their respective
/// short-duration locks; callers must not hold any of those locks when
/// calling.
pub fn build_inbox(
    store: &Arc<Mutex<PodcastStore>>,
    dismissed: &Arc<Mutex<HashSet<String>>>,
    triage_cache: &Arc<Mutex<HashMap<String, TriageResult>>>,
) -> Vec<InboxItem> {
    let dismissed_snapshot: HashSet<String> = match dismissed.lock() {
        Ok(d) => d.clone(),
        Err(_) => return Vec::new(),
    };

    let triage_snapshot: HashMap<String, TriageResult> = match triage_cache.lock() {
        Ok(c) => c.clone(),
        Err(_) => HashMap::new(),
    };

    let store_guard = match store.lock() {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let now = Utc::now().timestamp();
    let mut items: Vec<InboxItem> = Vec::new();

    for (podcast, episodes) in store_guard.all_podcasts() {
        for ep in episodes {
            if ep.played {
                continue;
            }
            let ep_id = ep.id.0.to_string();
            if dismissed_snapshot.contains(&ep_id) {
                continue;
            }

            let published_at = ep.pub_date.timestamp();

            let (priority_score, priority_reason, ai_categories) =
                if let Some(tr) = triage_snapshot.get(&ep_id) {
                    (tr.priority_score, tr.priority_reason.clone(), tr.categories.clone())
                } else {
                    let (s, r) = score(now, published_at);
                    (s, r.to_owned(), vec![])
                };

            items.push(InboxItem {
                episode_id: ep_id,
                episode_title: ep.title.clone(),
                podcast_id: podcast.id.0.to_string(),
                podcast_title: podcast.title.clone(),
                artwork_url: ep
                    .image_url
                    .as_ref()
                    .map(|u| u.to_string())
                    .or_else(|| podcast.image_url.as_ref().map(|u| u.to_string())),
                published_at,
                duration_secs: ep.duration_secs,
                priority_score,
                priority_reason: Some(priority_reason),
                ai_categories,
            });
        }
    }

    // Highest score first; ties broken newest-first so the visible order
    // is deterministic when many episodes published near the same time.
    items.sort_by(|a, b| {
        b.priority_score
            .partial_cmp(&a.priority_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.published_at.cmp(&a.published_at))
    });
    items
}

/// Recency-weighted heuristic: score newer episodes higher.
///
/// Returns the score (`0.0..=1.0`) and a short human-readable bucket
/// label that the row caption renders verbatim. This is the fallback path
/// when no LLM triage result is available for an episode.
fn score(now_unix: i64, published_at_unix: i64) -> (f32, &'static str) {
    const ONE_HOUR: i64 = 3_600;
    const ONE_DAY: i64 = 24 * ONE_HOUR;
    const WINDOW_SECS: i64 = 30 * ONE_DAY;

    let age = (now_unix - published_at_unix).max(0);
    if age < 12 * ONE_HOUR {
        return (1.0, "Just published");
    }
    if age < 3 * ONE_DAY {
        return (0.85, "Recent");
    }
    if age < 7 * ONE_DAY {
        return (0.65, "This week");
    }
    if age < WINDOW_SECS {
        // Linear taper from 0.55 down to 0.20 across the rest of the window.
        let progress = (age - 7 * ONE_DAY) as f32 / (WINDOW_SECS - 7 * ONE_DAY) as f32;
        let score = 0.55 - progress.clamp(0.0, 1.0) * 0.35;
        return (score, "From your library");
    }
    // Long-tail: keep a small floor so the inbox stays useful when the
    // user is on a catch-up binge against an old show.
    (0.15, "From your library")
}

/// Handle a `podcast.inbox.*` action and return the JSON envelope the FFI
/// surface emits back to Swift.
///
/// `Triage` runs LLM scoring on all unlistened episodes, writing results
/// to `triage_cache`, then bumps `rev` so the next snapshot tick picks up
/// the new scores. Episodes where the LLM call fails are left without a
/// cache entry (heuristic fallback applies on the next `build_inbox`).
///
/// `Dismiss` records the episode id in the dismissed set; the next tick's
/// `build_inbox` filters it out.
///
/// `MarkListened` flips `Episode.played = true` in the store; the next
/// tick's `build_inbox` filters it out (same code path as natural play-to-
/// completion).
pub fn handle_inbox_action(
    action: InboxAction,
    store: &Arc<Mutex<PodcastStore>>,
    dismissed: &Arc<Mutex<HashSet<String>>>,
    rev: &Arc<AtomicU64>,
    triage_cache: &Arc<Mutex<HashMap<String, TriageResult>>>,
    runtime: &Arc<Runtime>,
    in_progress: &Arc<std::sync::atomic::AtomicBool>,
) -> serde_json::Value {
    match action {
        InboxAction::Triage => {
            // Guard against concurrent triage passes (re-entrancy: user double-tap,
            // or an auto-trigger while the first pass is still running). If the flag
            // is already `true` a pass is in flight — return early rather than
            // spawning a second one that would race on the shared triage_cache.
            if in_progress.compare_exchange(
                false, true,
                std::sync::atomic::Ordering::Acquire,
                std::sync::atomic::Ordering::Relaxed,
            ).is_err() {
                return serde_json::json!({"ok": true, "status": "already_running"});
            }

            // M5.1: move triage off the actor thread. Prime the spinner rev now;
            // each per-episode result bumps `rev` again for incremental updates.
            rev.fetch_add(1, Ordering::Relaxed);

            let store_c = Arc::clone(store);
            let cache_c = Arc::clone(triage_cache);
            let runtime_c = Arc::clone(runtime);
            let rev_c = Arc::clone(rev);
            let in_progress_c = Arc::clone(in_progress);

            runtime.spawn(async move {
                triage_episodes_in_background(store_c, cache_c, runtime_c, rev_c, in_progress_c).await;
            });

            serde_json::json!({"ok": true, "status": "triage_started"})
        }
        InboxAction::Dismiss { episode_id } => match dismissed.lock() {
            Ok(mut d) => {
                d.insert(episode_id);
                rev.fetch_add(1, Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            Err(_) => serde_json::json!({"ok": false, "error": "dismissed_set poisoned"}),
        },
        InboxAction::MarkListened { episode_id } => match store.lock() {
            Ok(mut s) => {
                let _flipped = s.mark_episode_played(&episode_id);
                rev.fetch_add(1, Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            Err(_) => serde_json::json!({"ok": false, "error": "store poisoned"}),
        },
        InboxAction::MarkUnlistened { episode_id } => match store.lock() {
            Ok(mut s) => {
                let _flipped = s.mark_episode_unplayed(&episode_id);
                rev.fetch_add(1, Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            Err(_) => serde_json::json!({"ok": false, "error": "store poisoned"}),
        },
    }
}

/// Background async triage task (M5.1). Runs off the actor thread so the
/// kernel is never blocked waiting for Ollama. Each successful result bumps
/// `rev` so the iOS snapshot delivers incremental progress.
async fn triage_episodes_in_background(
    store: Arc<Mutex<PodcastStore>>,
    triage_cache: Arc<Mutex<HashMap<String, TriageResult>>>,
    runtime: Arc<Runtime>,
    rev: Arc<AtomicU64>,
    in_progress: Arc<std::sync::atomic::AtomicBool>,
) {
    // Collect episode metadata under a brief store lock then release it.
    let episodes_to_triage: Vec<(String, String, String, String)> = {
        let guard = match store.lock() {
            Ok(g) => g,
            Err(_) => {
                in_progress.store(false, std::sync::atomic::Ordering::Relaxed);
                return;
            }
        };
        guard
            .all_podcasts()
            .into_iter()
            .flat_map(|(podcast, eps)| {
                let pod_title = podcast.title.clone();
                eps.into_iter()
                    .filter(|e| !e.played)
                    .map(move |e| {
                        let ep_id = e.id.0.to_string();
                        let ep_title = e.title.clone();
                        let description: String = e.description.chars().take(500).collect();
                        (ep_id, ep_title, pod_title.clone(), description)
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    };

    // Process each episode sequentially; `triage_episode` itself drives the
    // async LLM call without `block_on` — we call `tokio::task::spawn_blocking`
    // to offload the synchronous rig-core call to the blocking thread pool.
    for (ep_id, ep_title, pod_title, description) in episodes_to_triage {
        let runtime2 = Arc::clone(&runtime);
        let ep_title2 = ep_title.clone();
        let pod_title2 = pod_title.clone();
        let description2 = description.clone();

        let result = tokio::task::spawn_blocking(move || {
            triage_episode(&ep_title2, &pod_title2, &description2, &runtime2)
        })
        .await;

        match result {
            Ok(Ok(triage)) => {
                if let Ok(mut cache) = triage_cache.lock() {
                    cache.insert(ep_id, triage);
                }
                // Bump rev so iOS picks up this result immediately.
                rev.fetch_add(1, Ordering::Relaxed);
            }
            Ok(Err(e)) => {
                eprintln!("[inbox_triage] LLM triage failed for {ep_id}: {e}");
            }
            Err(e) => {
                eprintln!("[inbox_triage] spawn_blocking panicked for {ep_id}: {e}");
            }
        }
    }

    // Clear the in-progress flag and emit a final rev bump.
    in_progress.store(false, std::sync::atomic::Ordering::Relaxed);
    rev.fetch_add(1, Ordering::Relaxed);
}

#[cfg(test)]
#[path = "inbox_handler_tests.rs"]
mod tests;
