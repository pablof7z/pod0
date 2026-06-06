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
//! `build_inbox` checks the LLM triage cache first. A `Ready` entry's
//! `priority_score`, `priority_reason`, and `categories` are used verbatim;
//! otherwise (no entry, or a `Pending` failure placeholder) the recency-bucket
//! heuristic (`score()`) is the fallback so the inbox always renders something
//! useful before the first triage pass or when Ollama is offline. `build_inbox`
//! is a **pure projection**: it never spawns work. The proactive population
//! lives in [`maybe_enqueue_triage`], called next to it in the snapshot builder.
//!
//! ## Proactive triage trigger
//!
//! Triage no longer waits for an explicit user `InboxAction::Triage`. On each
//! snapshot tick the snapshot builder calls [`maybe_enqueue_triage`], which
//! asks the pure [`episodes_needing_triage`] predicate whether any unlistened
//! episode lacks a fresh `Ready` entry. If so — and no pass is already in
//! flight — it spawns the same background task the explicit action uses, so
//! both paths share the `in_progress` re-entrancy guard and can never race.
//! The predicate is conservative (no entry, a `Ready` entry older than
//! [`TRIAGE_STALE_SECS`], or a `Pending` entry older than
//! [`TRIAGE_RETRY_COOLDOWN_SECS`]) so an offline Ollama degrades to the
//! heuristic instead of a hot spawn loop.
//!
//! All LLM work runs off the actor thread (`runtime.spawn` →
//! `tokio::task::spawn_blocking`); the actor is never blocked. After a batch
//! completes, the cache is persisted via
//! [`crate::store::inbox_triage_cache`] so a cold launch reloads prior scores.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use tokio::runtime::Runtime;

use crate::agent_llm;
use crate::agent_tools::ToolRegistry;
use crate::ffi::actions::inbox_module::InboxAction;
use crate::ffi::projections::InboxItem;
use crate::inbox_llm::{TriageResult, TriageStatus};
use crate::store::PodcastStore;

/// A `Ready` triage entry older than this is considered stale and re-triaged
/// by the proactive trigger (the episode's metadata or the model may have
/// changed, and scores drift as "recent" decays). 24 hours per spec.
const TRIAGE_STALE_SECS: i64 = 24 * 60 * 60;

/// A `Pending` (failed) triage entry is not retried until this cooldown
/// elapses. Without it, an offline Ollama would make `episodes_needing_triage`
/// return `true` on *every* snapshot tick, hot-looping `spawn_blocking` against
/// a dead endpoint. 10 minutes balances "retries next time" against that loop.
const TRIAGE_RETRY_COOLDOWN_SECS: i64 = 10 * 60;

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

    for (podcast, episodes) in store_guard.subscribed_podcasts() {
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
            .subscribed_podcasts()
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
                // Delete-after-played is kernel-owned policy (D0). A manual
                // mark-played (and the sleep-timer-end path, which routes
                // through `inbox/mark_listened`) honours the user's
                // `auto_delete_downloads_after_played` setting here. Keyed only
                // on the auto-delete setting (not `auto_mark_played_at_end`),
                // matching the prior Swift `markEpisodePlayed` gate. File
                // removal stays out of the store, mirroring
                // `handle_delete_download`.
                if let Some(path) = s.clear_local_path_if_auto_delete(&episode_id) {
                    let _ = std::fs::remove_file(&path);
                }
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

/// Background triage task. Runs off the actor thread via `spawn_blocking`.
///
/// Instantiates the same agent the user chats with (same identity + memory
/// facts) and sends it a single user message listing all needy episodes.
/// The agent calls `set_episode_priorities` to write scores directly into
/// `triage_cache`. After the agent returns, any needy episode still missing
/// a `Ready` entry gets `stamp_pending` so the cooldown applies and the
/// proactive trigger doesn't hot-loop.
///
/// Cold-start guard: if the user has no memory facts AND no played/starred
/// episodes, the agent has nothing to personalize on — skip the LLM and
/// let the heuristic carry the inbox until there is real signal.
async fn triage_episodes_in_background(
    store: Arc<Mutex<PodcastStore>>,
    triage_cache: Arc<Mutex<HashMap<String, TriageResult>>>,
    runtime: Arc<Runtime>,
    rev: Arc<AtomicU64>,
    in_progress: Arc<std::sync::atomic::AtomicBool>,
) {
    // Collect needy episode metadata + cold-start signals under a brief store lock.
    struct EpisodeInput {
        ep_id: String,
        ep_title: String,
        pod_title: String,
        pub_date: String,
    }
    let (episodes, has_signal) = {
        let guard = match store.lock() {
            Ok(g) => g,
            Err(_) => {
                in_progress.store(false, std::sync::atomic::Ordering::Relaxed);
                return;
            }
        };

        let has_memory = !guard.all_memory_facts().is_empty();
        let has_history = guard.subscribed_podcasts().into_iter().any(|(_, eps)| {
            eps.iter().any(|e| e.played || e.is_starred || e.position_secs > 0.0)
        });

        let eps: Vec<EpisodeInput> = guard
            .subscribed_podcasts()
            .into_iter()
            .flat_map(|(podcast, eps)| {
                let pod_title = podcast.title.clone();
                eps.iter()
                    .filter(|e| !e.played)
                    .map(move |e| EpisodeInput {
                        ep_id: e.id.0.to_string(),
                        ep_title: e.title.clone(),
                        pod_title: pod_title.clone(),
                        pub_date: e.pub_date.format("%Y-%m-%d").to_string(),
                    })
                    .collect::<Vec<_>>()
            })
            .collect();

        (eps, has_memory || has_history)
    };

    if episodes.is_empty() {
        in_progress.store(false, std::sync::atomic::Ordering::Relaxed);
        return;
    }

    // Cold-start: no user signal → skip LLM, stamp all episodes Pending so the
    // cooldown applies until the user has listened to something.
    if !has_signal {
        let now = Utc::now().timestamp();
        if let Ok(mut cache) = triage_cache.lock() {
            for ep in &episodes {
                cache.entry(ep.ep_id.clone()).or_insert_with(|| TriageResult::pending(now));
            }
        }
        in_progress.store(false, std::sync::atomic::Ordering::Relaxed);
        rev.fetch_add(1, Ordering::Relaxed);
        return;
    }

    // Build the needy-episode id set before the agent call so we can reconcile
    // gaps after it returns.
    let needy_ids: Vec<String> = episodes.iter().map(|e| e.ep_id.clone()).collect();

    // Compose the user message: all episodes in a single agent invocation.
    let episode_lines: String = episodes
        .iter()
        .map(|e| {
            format!(
                "- episode_id: {} | podcast: \"{}\" | title: \"{}\" | published: {}",
                e.ep_id, e.pod_title, e.ep_title, e.pub_date
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let user_message = format!(
        "Please prioritize my inbox. Here are the new episodes since we last checked.\n\
         Score each one 0.0–1.0 for how much I would want to hear it, give a one-sentence \
         reason referencing my interests, and tag relevant categories. \
         Use get_memory_facts and search_library to understand my taste, then record \
         all scores with a single set_episode_priorities call.\n\n\
         Episodes:\n{episode_lines}"
    );

    // Build the system prompt using the same agent identity + memory facts.
    let system_prompt = agent_llm::build_system_prompt_with_memory(Some(&store));
    let registry = ToolRegistry::for_triage(
        Arc::clone(&store),
        Arc::clone(&triage_cache),
        Arc::clone(&rev),
    );

    let store_c = Arc::clone(&store);
    let runtime_c = Arc::clone(&runtime);

    let outcome = tokio::task::spawn_blocking(move || {
        agent_llm::run_background_agent_task(
            &system_prompt,
            &user_message,
            store_c,
            registry,
            &runtime_c,
        )
    })
    .await;

    if let Err(e) = &outcome {
        eprintln!("[inbox_triage] spawn_blocking panicked: {e}");
    }
    if let Ok(Err(ref e)) = outcome {
        eprintln!("[inbox_triage] agent call failed: {e}");
    }

    // Reconcile: any needy episode still missing a Ready entry after the agent
    // call gets stamp_pending so the cooldown applies. This covers partial
    // failures (agent skipped some episodes) and total failures (agent errored).
    reconcile_pending(&triage_cache, &needy_ids);

    // Persist the cache so a cold launch reloads these scores.
    crate::store::inbox_triage_cache::persist_from_store(&store, &triage_cache);

    in_progress.store(false, std::sync::atomic::Ordering::Relaxed);
    rev.fetch_add(1, Ordering::Relaxed);
}

/// Stamp `Pending` for every episode in `needy_ids` that still lacks a fresh
/// `Ready` entry. Preserves existing `Ready` entries (including ones just
/// written by `set_episode_priorities`) and updates their `attempted_at` only
/// when the entry is already `Pending` (avoids downgrading a good score).
fn reconcile_pending(triage_cache: &Arc<Mutex<HashMap<String, TriageResult>>>, needy_ids: &[String]) {
    for ep_id in needy_ids {
        stamp_pending(triage_cache, ep_id.clone());
    }
}

/// Stamp a single episode's triage-cache entry as `Pending`, refreshing the
/// retry cooldown (`attempted_at` → now) — UNLESS it already holds a `Ready`
/// score, which is left untouched so a failed re-triage never downgrades a good
/// score. This is the per-episode body of [`reconcile_pending`]; kept as its own
/// fn so the behavior has one source of truth (and is unit-testable in isolation).
fn stamp_pending(triage_cache: &Arc<Mutex<HashMap<String, TriageResult>>>, ep_id: String) {
    let now = Utc::now().timestamp();
    if let Ok(mut cache) = triage_cache.lock() {
        match cache.get(&ep_id) {
            Some(tr) if tr.status == TriageStatus::Ready => {
                // Agent wrote a good score — leave it alone.
            }
            _ => {
                // Missing or still Pending — stamp/refresh the cooldown.
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
