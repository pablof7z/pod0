//! Picks-projection compute + slot writeback for the `podcast.picks` action
//! namespace.
//!
//! Extracted into its own file so `host_op_handler.rs` stays under the 500-line
//! hard cap and so the heuristic-vs-LLM swap-out point is obvious.
//!
//! The store→candidate translation lives here (not in `picks_module.rs`) so the
//! pure heuristic stays decoupled from `PodcastStore` internals; this file is
//! the only consumer that knows how to walk the store.
//!
//! ## Scoring strategy (M5.6)
//!
//! Picks materializes a slot once per refresh; the snapshot reader just reads
//! that slot (unlike inbox, which recomputes per tick). So LLM scoring cannot
//! merely populate a cache and bump `rev` — it must **re-stamp the slot**.
//!
//! `handle_refresh` therefore does two writes:
//!   1. **Immediately** stamp the newest-first heuristic so the rail fills with
//!      zero latency (and so the synchronous feed-refresh call sites that use
//!      [`refresh_picks_into_slot`] keep working unchanged).
//!   2. **In the background**, score each candidate via Ollama
//!      ([`score_episode_for_picks`]) off the actor thread, then re-stamp the
//!      slot with [`compute_picks_scored`] and bump `rev` so iOS observes the
//!      upgraded picks.
//!
//! If Ollama is offline every per-episode call returns `Err` and
//! `compute_picks_scored` falls back to the recency heuristic for those
//! candidates — so the worst case degrades to the original behavior.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::runtime::Runtime;

use crate::ffi::actions::picks_module::{compute_picks, compute_picks_scored, CandidateEpisode};
use crate::ffi::projections::AgentPickSummary;
use crate::picks_llm::score_episode_for_picks;
use crate::store::PodcastStore;

/// Recompute the picks slot from the current `PodcastStore` contents using the
/// newest-first heuristic, stamp it onto the shared `picks_slot`, and bump
/// `rev` so the next iOS snapshot observes the change.
///
/// This is the synchronous, no-LLM path. It is called directly by the
/// feed-refresh sites (`podcast_actions_feed.rs`) and as the immediate first
/// write inside [`handle_refresh`].
///
/// Lock discipline: the store is locked only long enough to drain it into a
/// flat `Vec<CandidateEpisode>`. The picks slot is then locked separately for
/// the write — never both at once. Failure (poisoned locks) degrades silently
/// per D6.
pub fn refresh_picks_into_slot(
    store: &Arc<Mutex<PodcastStore>>,
    picks_slot: &Arc<Mutex<Vec<AgentPickSummary>>>,
    rev: &Arc<AtomicU64>,
) {
    let candidates = match store.lock() {
        Ok(s) => collect_candidates(&s),
        Err(_) => return,
    };
    let picks = compute_picks(candidates);
    if let Ok(mut slot) = picks_slot.lock() {
        *slot = picks;
        rev.fetch_add(1, Ordering::Relaxed);
    }
}

/// Flatten the store into the heuristic's input shape.
///
/// Iterates every subscribed podcast + every episode. The heuristic itself
/// decides ordering + caps; we just hand it the raw set.
fn collect_candidates(store: &PodcastStore) -> Vec<CandidateEpisode> {
    let mut out: Vec<CandidateEpisode> = Vec::new();
    for (podcast, episodes) in store.all_podcasts() {
        let podcast_id = podcast.id.0.to_string();
        let podcast_title = podcast.title.clone();
        let show_art = podcast.image_url.as_ref().map(|u| u.to_string());
        for ep in episodes {
            let ep_art = ep
                .image_url
                .as_ref()
                .map(|u| u.to_string())
                .or_else(|| show_art.clone());
            out.push(CandidateEpisode {
                episode_id: ep.id.0.to_string(),
                episode_title: ep.title.clone(),
                podcast_id: podcast_id.clone(),
                podcast_title: podcast_title.clone(),
                artwork_url: ep_art,
                published_at: ep.pub_date.timestamp(),
                duration_secs: ep.duration_secs,
            });
        }
    }
    out
}

/// Collect the per-episode metadata the LLM scorer needs: the prompt inputs
/// `(episode_id, episode_title, podcast_title, description)`.
///
/// Kept separate from [`collect_candidates`] because `CandidateEpisode` does
/// not carry the description (it is not part of the projection), and the
/// scorer only needs these four fields. Descriptions are truncated to 500
/// chars — matching the inbox triage path — to bound prompt size.
fn collect_score_inputs(store: &PodcastStore) -> Vec<(String, String, String, String)> {
    let mut out: Vec<(String, String, String, String)> = Vec::new();
    for (podcast, episodes) in store.all_podcasts() {
        let podcast_title = podcast.title.clone();
        for ep in episodes {
            let description: String = ep.description.chars().take(500).collect();
            out.push((
                ep.id.0.to_string(),
                ep.title.clone(),
                podcast_title.clone(),
                description,
            ));
        }
    }
    out
}

/// Handler for `{"op":"refresh"}` on the `podcast.picks` namespace.
///
/// Stamps the heuristic immediately (so the UI is never empty while the LLM
/// runs), then spawns a background task that scores candidates via Ollama and
/// re-stamps the slot. Returns the `{"ok":true}` envelope every host-op
/// handler returns.
///
/// `in_progress` guards against concurrent refresh passes: Refresh can be
/// dispatched repeatedly (user pull-to-refresh, auto-trigger after feed sync),
/// and two background passes would race on the materialized slot. A second
/// dispatch while one is in flight returns immediately (the heuristic is still
/// re-stamped so the rail stays current).
pub fn handle_refresh(
    store: &Arc<Mutex<PodcastStore>>,
    picks_slot: &Arc<Mutex<Vec<AgentPickSummary>>>,
    rev: &Arc<AtomicU64>,
    runtime: &Arc<Runtime>,
    in_progress: &Arc<AtomicBool>,
) -> serde_json::Value {
    // 1. Immediate heuristic stamp — the rail fills with zero latency.
    refresh_picks_into_slot(store, picks_slot, rev);

    // 2. Re-entrancy guard: if a scoring pass is already running, the fresh
    //    heuristic stamp above is enough; don't spawn a racing second pass.
    if in_progress
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        return serde_json::json!({"ok": true, "status": "already_running"});
    }

    let store_c = Arc::clone(store);
    let picks_c = Arc::clone(picks_slot);
    let rev_c = Arc::clone(rev);
    let runtime_c = Arc::clone(runtime);
    let in_progress_c = Arc::clone(in_progress);

    runtime.spawn(async move {
        score_picks_in_background(store_c, picks_c, rev_c, runtime_c, in_progress_c).await;
    });

    serde_json::json!({"ok": true, "status": "scoring_started"})
}

/// Background async picks-scoring task (M5.6). Runs off the actor thread so the
/// kernel is never blocked waiting for Ollama.
///
/// Scores every candidate sequentially via `spawn_blocking` (the inbox-triage
/// nesting), accumulates `(score, reason)` per `episode_id`, then re-stamps the
/// slot with [`compute_picks_scored`] and bumps `rev`. Candidates whose LLM
/// call fails are simply absent from the score map; `compute_picks_scored`
/// falls them back to the recency heuristic.
async fn score_picks_in_background(
    store: Arc<Mutex<PodcastStore>>,
    picks_slot: Arc<Mutex<Vec<AgentPickSummary>>>,
    rev: Arc<AtomicU64>,
    runtime: Arc<Runtime>,
    in_progress: Arc<AtomicBool>,
) {
    // Snapshot candidates + score inputs under a brief store lock, then release.
    let (candidates, score_inputs) = {
        let guard = match store.lock() {
            Ok(g) => g,
            Err(_) => {
                in_progress.store(false, Ordering::Relaxed);
                return;
            }
        };
        (collect_candidates(&guard), collect_score_inputs(&guard))
    };

    let mut scores: HashMap<String, (f32, String)> = HashMap::new();
    for (ep_id, ep_title, pod_title, description) in score_inputs {
        let runtime2 = Arc::clone(&runtime);
        let result = tokio::task::spawn_blocking(move || {
            score_episode_for_picks(&ep_title, &pod_title, &description, &runtime2)
        })
        .await;

        match result {
            Ok(Ok((score, reason))) => {
                scores.insert(ep_id, (score, reason));
            }
            Ok(Err(e)) => {
                eprintln!("[picks_llm] scoring failed for {ep_id}: {e}");
            }
            Err(e) => {
                eprintln!("[picks_llm] spawn_blocking panicked for {ep_id}: {e}");
            }
        }
    }

    // Re-stamp the slot with the upgraded scores (heuristic fallback for any
    // unscored candidate) and bump rev so iOS picks up the change.
    let upgraded = compute_picks_scored(candidates, &scores);
    if let Ok(mut slot) = picks_slot.lock() {
        *slot = upgraded;
    }
    rev.fetch_add(1, Ordering::Relaxed);
    in_progress.store(false, Ordering::Relaxed);
}

#[cfg(test)]
#[path = "picks_handler_tests.rs"]
mod tests;
