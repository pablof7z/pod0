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
//! `build_inbox` checks the LLM triage cache first. If a `Ready` cache entry
//! exists for an episode, its `priority_score`, `priority_reason`, and
//! `categories` are used verbatim. Otherwise (no entry, or a `Pending`
//! failure-placeholder entry) the recency-bucket heuristic (`score()`) is the
//! fallback so the inbox always renders something useful even before the first
//! triage pass completes or when Ollama is offline.
//!
//! `build_inbox` is a **pure projection**: it never spawns work. The proactive
//! population lives in [`maybe_enqueue_triage`], called from the snapshot
//! builder right next to `build_inbox`.
//!
//! ## Proactive triage trigger
//!
//! Triage no longer waits for an explicit user `InboxAction::Triage`. On each
//! snapshot tick the snapshot builder calls [`maybe_enqueue_triage`], which
//! asks the pure [`episodes_needing_triage`] predicate whether any unlistened
//! episode lacks a fresh `Ready` entry. If so — and no pass is already in
//! flight — it spawns the same background task the explicit action uses, so
//! both paths share the `in_progress` re-entrancy guard and can never race.
//!
//! The predicate is deliberately conservative to avoid hammering Ollama every
//! tick: an episode needs triage when it has **no** entry, a **`Ready`** entry
//! older than [`TRIAGE_STALE_SECS`], or a **`Pending`** entry older than
//! [`TRIAGE_RETRY_COOLDOWN_SECS`]. A recent `Pending` entry suppresses retries
//! during the cooldown so an offline Ollama degrades to the heuristic instead
//! of a hot spawn loop.
//!
//! All LLM work runs off the actor thread (`runtime.spawn` →
//! `tokio::task::spawn_blocking`); the actor is never blocked.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use tokio::runtime::Runtime;

use crate::ffi::actions::inbox_module::InboxAction;
use crate::ffi::projections::InboxItem;
use crate::agent_llm::base_url_from_chat_url;
use crate::inbox_llm::{triage_episode, TriageResult, TriageStatus};

/// A `Ready` triage entry older than this is considered stale and re-triaged
/// by the proactive trigger (the episode's metadata or the model may have
/// changed, and scores drift as "recent" decays). 24 hours per spec.
const TRIAGE_STALE_SECS: i64 = 24 * 60 * 60;

/// A `Pending` (failed) triage entry is not retried until this cooldown
/// elapses. Without it, an offline Ollama would make `episodes_needing_triage`
/// return `true` on *every* snapshot tick, hot-looping `spawn_blocking` against
/// a dead endpoint. 10 minutes balances "retries next time" against that loop.
const TRIAGE_RETRY_COOLDOWN_SECS: i64 = 10 * 60;
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

            // Only a `Ready` cache entry carries an authoritative LLM score.
            // A `Pending` entry is a failure placeholder (it exists only to
            // throttle retries) so it falls through to the recency heuristic,
            // exactly as a missing entry does.
            let (priority_score, priority_reason, ai_categories) = match triage_snapshot.get(&ep_id)
            {
                Some(tr) if tr.status == TriageStatus::Ready => {
                    (tr.priority_score, tr.priority_reason.clone(), tr.categories.clone())
                }
                _ => {
                    let (s, r) = score(now, published_at);
                    (s, r.to_owned(), vec![])
                }
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

/// Decide whether the proactive trigger should run a triage pass.
///
/// Pure function over the current set of unlistened `episode_ids` and a
/// snapshot of the triage cache. Returns `true` when **any** episode warrants
/// a (re)triage:
///
/// * no cache entry at all — never attempted;
/// * a [`TriageStatus::Ready`] entry older than [`TRIAGE_STALE_SECS`] — stale;
/// * a [`TriageStatus::Pending`] entry older than
///   [`TRIAGE_RETRY_COOLDOWN_SECS`] — failed, cooldown elapsed, retry.
///
/// A `Ready` entry within the staleness window, or a `Pending` entry within
/// the cooldown, suppresses the trigger — this is what keeps an offline Ollama
/// from re-spawning a background pass on every snapshot tick.
///
/// Kept pure (no locks, no spawn) so it is unit-testable without a Tokio
/// runtime, mirroring how #141 tested the synchronous triage prelude.
pub fn episodes_needing_triage(
    cache: &HashMap<String, TriageResult>,
    episode_ids: &[String],
    now_unix: i64,
) -> bool {
    episode_ids.iter().any(|id| match cache.get(id) {
        None => true,
        Some(tr) => match tr.status {
            TriageStatus::Ready => now_unix - tr.attempted_at >= TRIAGE_STALE_SECS,
            TriageStatus::Pending => now_unix - tr.attempted_at >= TRIAGE_RETRY_COOLDOWN_SECS,
        },
    })
}

/// Proactive triage trigger — the snapshot-path counterpart to the explicit
/// `InboxAction::Triage` user action.
///
/// Called from the snapshot builder right next to [`build_inbox`] (so it runs
/// once per tick). It collects the unlistened episode ids under a brief store
/// lock, releases it, and — if [`episodes_needing_triage`] says so and no pass
/// is already in flight — spawns the **same** [`triage_episodes_in_background`]
/// task the user action uses. Sharing the task and the `in_progress` guard
/// means the proactive and explicit paths can never run concurrently.
///
/// This never blocks the caller: the predicate check is cheap and the LLM work
/// happens on the spawned task. If a pass is already running, or nothing needs
/// triage, it returns without spawning. `rev` is **not** bumped here — the
/// background task bumps it as each result lands.
pub fn maybe_enqueue_triage(
    store: &Arc<Mutex<PodcastStore>>,
    triage_cache: &Arc<Mutex<HashMap<String, TriageResult>>>,
    rev: &Arc<AtomicU64>,
    runtime: &Arc<Runtime>,
    in_progress: &Arc<AtomicBool>,
) {
    // Cheap pre-check: if a pass is already running, the in-flight task will
    // populate the cache. Skip the store walk entirely.
    if in_progress.load(Ordering::Relaxed) {
        return;
    }

    // Collect unlistened episode ids under a brief store lock, then release.
    let episode_ids: Vec<String> = match store.lock() {
        Ok(guard) => guard
            .all_podcasts()
            .into_iter()
            .flat_map(|(_, eps)| {
                eps.iter()
                    .filter(|e| !e.played)
                    .map(|e| e.id.0.to_string())
                    .collect::<Vec<_>>()
            })
            .collect(),
        Err(_) => return,
    };

    if episode_ids.is_empty() {
        return;
    }

    let now = Utc::now().timestamp();
    let needs = match triage_cache.lock() {
        Ok(cache) => episodes_needing_triage(&cache, &episode_ids, now),
        Err(_) => return,
    };
    if !needs {
        return;
    }

    // Claim the in_progress guard; lose the race → another pass is starting,
    // so do nothing (the winner covers this tick).
    if in_progress
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        return;
    }

    let store_c = Arc::clone(store);
    let cache_c = Arc::clone(triage_cache);
    let runtime_c = Arc::clone(runtime);
    let rev_c = Arc::clone(rev);
    let in_progress_c = Arc::clone(in_progress);

    runtime.spawn(async move {
        triage_episodes_in_background(store_c, cache_c, runtime_c, rev_c, in_progress_c).await;
    });
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
    // Collect episode metadata and the configured Ollama URL under a brief store lock.
    let (episodes_to_triage, ollama_base_url): (Vec<(String, String, String, String)>, String) = {
        let guard = match store.lock() {
            Ok(g) => g,
            Err(_) => {
                in_progress.store(false, std::sync::atomic::Ordering::Relaxed);
                return;
            }
        };
        let base_url = base_url_from_chat_url(guard.ollama_chat_url());
        let episodes = guard
            .all_podcasts()
            .into_iter()
            .flat_map(|(podcast, eps)| {
                let pod_title = podcast.title.clone();
                eps.iter()
                    .filter(|e| !e.played)
                    .map(move |e| {
                        let ep_id = e.id.0.to_string();
                        let ep_title = e.title.clone();
                        let description: String = e.description.chars().take(500).collect();
                        (ep_id, ep_title, pod_title.clone(), description)
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        (episodes, base_url)
    };

    // Process each episode sequentially; `triage_episode` itself drives the
    // async LLM call without `block_on` — we call `tokio::task::spawn_blocking`
    // to offload the synchronous rig-core call to the blocking thread pool.
    for (ep_id, ep_title, pod_title, description) in episodes_to_triage {
        let runtime2 = Arc::clone(&runtime);
        let ep_title2 = ep_title.clone();
        let pod_title2 = pod_title.clone();
        let description2 = description.clone();
        let base_url2 = ollama_base_url.clone();

        let result = tokio::task::spawn_blocking(move || {
            triage_episode(&ep_title2, &pod_title2, &description2, &runtime2, &base_url2)
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
                // Stamp a Pending placeholder so build_inbox keeps the heuristic
                // fallback AND the proactive trigger waits out the retry cooldown
                // instead of re-spawning this pass on the very next tick.
                stamp_pending(&triage_cache, ep_id);
            }
            Err(e) => {
                eprintln!("[inbox_triage] spawn_blocking panicked for {ep_id}: {e}");
                stamp_pending(&triage_cache, ep_id);
            }
        }
    }

    // Clear the in-progress flag and emit a final rev bump.
    in_progress.store(false, std::sync::atomic::Ordering::Relaxed);
    rev.fetch_add(1, Ordering::Relaxed);
}

/// Record a failed triage attempt in the cache so the retry cooldown applies.
///
/// Two cases:
/// * **No entry, or an existing `Pending` entry** → write a fresh `Pending`
///   stamped now. `build_inbox` keeps using the heuristic; the proactive
///   trigger waits out [`TRIAGE_RETRY_COOLDOWN_SECS`] before retrying.
/// * **An existing `Ready` entry** (a stale re-triage that just failed) → keep
///   the good score but bump its `attempted_at` to now. A transient failure
///   must not downgrade a usable score to the heuristic, yet the staleness
///   clock has to reset or the trigger would re-spawn every tick.
fn stamp_pending(triage_cache: &Arc<Mutex<HashMap<String, TriageResult>>>, ep_id: String) {
    let now = Utc::now().timestamp();
    if let Ok(mut cache) = triage_cache.lock() {
        match cache.get_mut(&ep_id) {
            Some(tr) if tr.status == TriageStatus::Ready => {
                tr.attempted_at = now;
            }
            _ => {
                cache.insert(ep_id, TriageResult::pending(now));
            }
        }
    }
}

#[cfg(test)]
#[path = "inbox_handler_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "inbox_proactive_tests.rs"]
mod proactive_tests;
