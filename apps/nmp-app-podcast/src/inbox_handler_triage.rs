//! Inbox triage **actions** and the background LLM scoring pass.
//!
//! Split out of `inbox_handler.rs` (which kept the pure projection —
//! [`super::build_inbox`] / [`super::episodes_needing_triage`] / `score`) so
//! both halves stay under the 500-line hard limit. This half owns everything
//! that *mutates*: the `podcast.inbox.*` action handlers
//! ([`handle_inbox_action`]), the proactive snapshot-tick trigger
//! ([`maybe_enqueue_triage`]), and the off-actor-thread agent task
//! ([`triage_episodes_in_background`]) plus its `Pending`-reconciliation
//! helpers.
//!
//! Re-exported from `inbox_handler` so external callers keep using the
//! `crate::inbox_handler::{handle_inbox_action, maybe_enqueue_triage, …}`
//! paths unchanged.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use tokio::runtime::Runtime;

use super::episodes_needing_triage;
use crate::agent_llm;
use crate::agent_tools::ToolRegistry;
use crate::ffi::actions::inbox_module::InboxAction;
use crate::inbox_llm::{TriageResult, TriageStatus};
use crate::snapshot_signal::SnapshotUpdateSignal;
use crate::store::PodcastStore;

/// Proactive triage trigger — the snapshot-path counterpart to the explicit
/// `InboxAction::Triage` user action.
///
/// Called from the snapshot builder right next to [`super::build_inbox`] (so it
/// runs once per tick). It collects the unlistened episode ids under a brief
/// store lock, releases it, and — if [`episodes_needing_triage`] says so and no
/// pass is already in flight — spawns the **same** [`triage_episodes_in_background`]
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
    maybe_enqueue_triage_inner(store, triage_cache, rev, runtime, in_progress, None);
}

pub fn maybe_enqueue_triage_with_signal(
    store: &Arc<Mutex<PodcastStore>>,
    triage_cache: &Arc<Mutex<HashMap<String, TriageResult>>>,
    rev: &Arc<AtomicU64>,
    runtime: &Arc<Runtime>,
    in_progress: &Arc<AtomicBool>,
    snapshot_signal: SnapshotUpdateSignal,
) {
    maybe_enqueue_triage_inner(
        store,
        triage_cache,
        rev,
        runtime,
        in_progress,
        Some(snapshot_signal),
    );
}

fn maybe_enqueue_triage_inner(
    store: &Arc<Mutex<PodcastStore>>,
    triage_cache: &Arc<Mutex<HashMap<String, TriageResult>>>,
    rev: &Arc<AtomicU64>,
    runtime: &Arc<Runtime>,
    in_progress: &Arc<AtomicBool>,
    snapshot_signal: Option<SnapshotUpdateSignal>,
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
        triage_episodes_in_background(
            store_c,
            cache_c,
            runtime_c,
            rev_c,
            in_progress_c,
            snapshot_signal,
        )
        .await;
    });
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
    handle_inbox_action_inner(
        action,
        store,
        dismissed,
        rev,
        triage_cache,
        runtime,
        in_progress,
        None,
    )
}

pub fn handle_inbox_action_with_signal(
    action: InboxAction,
    store: &Arc<Mutex<PodcastStore>>,
    dismissed: &Arc<Mutex<HashSet<String>>>,
    rev: &Arc<AtomicU64>,
    triage_cache: &Arc<Mutex<HashMap<String, TriageResult>>>,
    runtime: &Arc<Runtime>,
    in_progress: &Arc<std::sync::atomic::AtomicBool>,
    snapshot_signal: SnapshotUpdateSignal,
) -> serde_json::Value {
    handle_inbox_action_inner(
        action,
        store,
        dismissed,
        rev,
        triage_cache,
        runtime,
        in_progress,
        Some(snapshot_signal),
    )
}

fn handle_inbox_action_inner(
    action: InboxAction,
    store: &Arc<Mutex<PodcastStore>>,
    dismissed: &Arc<Mutex<HashSet<String>>>,
    rev: &Arc<AtomicU64>,
    triage_cache: &Arc<Mutex<HashMap<String, TriageResult>>>,
    runtime: &Arc<Runtime>,
    in_progress: &Arc<std::sync::atomic::AtomicBool>,
    snapshot_signal: Option<SnapshotUpdateSignal>,
) -> serde_json::Value {
    match action {
        InboxAction::Triage => {
            // Guard against concurrent triage passes (re-entrancy: user double-tap,
            // or an auto-trigger while the first pass is still running). If the flag
            // is already `true` a pass is in flight — return early rather than
            // spawning a second one that would race on the shared triage_cache.
            if in_progress
                .compare_exchange(
                    false,
                    true,
                    std::sync::atomic::Ordering::Acquire,
                    std::sync::atomic::Ordering::Relaxed,
                )
                .is_err()
            {
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
                triage_episodes_in_background(
                    store_c,
                    cache_c,
                    runtime_c,
                    rev_c,
                    in_progress_c,
                    snapshot_signal,
                )
                .await;
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
                let flipped = s.mark_episode_played(&episode_id);
                // Playback completion is the authoritative "I finished this
                // episode" signal. Every completion route — manual mark, natural
                // play-to-end, and the sleep-timer end — funnels through
                // `inbox/mark_listened`, so recording it here covers them all in
                // one place. Only on a genuine unplayed→played flip, so a repeat
                // mark on an already-played episode doesn't pile duplicate rows.
                if flipped {
                    s.emit_event_simple(
                        &episode_id,
                        crate::store::events::stage::PLAYBACK_COMPLETED,
                        crate::store::events::EventSeverity::Success,
                        "Marked played",
                    );
                }
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
    snapshot_signal: Option<SnapshotUpdateSignal>,
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
            eps.iter()
                .any(|e| e.played || e.is_starred || e.position_secs > 0.0)
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
                cache
                    .entry(ep.ep_id.clone())
                    .or_insert_with(|| TriageResult::pending(now));
            }
        }
        in_progress.store(false, std::sync::atomic::Ordering::Relaxed);
        bump_background_rev(&rev, snapshot_signal.as_ref());
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
    let registry = match snapshot_signal.clone() {
        Some(signal) => ToolRegistry::for_triage_with_signal(
            Arc::clone(&store),
            Arc::clone(&triage_cache),
            Arc::clone(&rev),
            signal,
        ),
        None => ToolRegistry::for_triage(
            Arc::clone(&store),
            Arc::clone(&triage_cache),
            Arc::clone(&rev),
        ),
    };

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
    bump_background_rev(&rev, snapshot_signal.as_ref());
}

fn bump_background_rev(rev: &AtomicU64, snapshot_signal: Option<&SnapshotUpdateSignal>) {
    if let Some(signal) = snapshot_signal {
        signal.bump();
    } else {
        rev.fetch_add(1, Ordering::Relaxed);
    }
}

/// Stamp `Pending` for every episode in `needy_ids` that still lacks a fresh
/// `Ready` entry. Preserves existing `Ready` entries (including ones just
/// written by `set_episode_priorities`) and updates their `attempted_at` only
/// when the entry is already `Pending` (avoids downgrading a good score).
fn reconcile_pending(
    triage_cache: &Arc<Mutex<HashMap<String, TriageResult>>>,
    needy_ids: &[String],
) {
    for ep_id in needy_ids {
        stamp_pending(triage_cache, ep_id.clone());
    }
}

/// Stamp a single episode's triage-cache entry as `Pending`, refreshing the
/// retry cooldown (`attempted_at` → now) — UNLESS it already holds a `Ready`
/// score, which is left untouched so a failed re-triage never downgrades a good
/// score. This is the per-episode body of [`reconcile_pending`]; kept as its own
/// fn so the behavior has one source of truth (and is unit-testable in isolation).
pub(crate) fn stamp_pending(
    triage_cache: &Arc<Mutex<HashMap<String, TriageResult>>>,
    ep_id: String,
) {
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
