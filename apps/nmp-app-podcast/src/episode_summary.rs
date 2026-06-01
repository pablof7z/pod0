//! Actor-thread glue for episode summarization
//! ([`PodcastAction::SummarizeEpisode`]).
//!
//! [`handle_summarize_episode`] is the single-episode analogue of
//! [`crate::categorization::handle_run`]'s background LLM pass:
//!
//! 1. Read the episode's `(title, description, transcript)` inputs under a
//!    short store lock, then drop the lock.
//! 2. Spawn an async task on the shared runtime that runs the Ollama call
//!    inside `spawn_blocking` (so the kernel actor thread is never blocked).
//! 3. On success, stamp the summary onto the episode via
//!    [`crate::store::PodcastStore::set_episode_summary`] (which persists) and
//!    bump `rev` so the snapshot projection delivers the new `summary` to the
//!    iOS shell. On failure, log and leave the episode untouched — there is no
//!    cheap heuristic fallback worth persisting (see [`crate::episode_summary_llm`]).
//!
//! The action is fire-and-forget at the dispatch envelope level
//! (`PodcastActionModule::is_async_completing() == false`): the host op returns
//! `{"ok":true,"status":"summarizing"}` immediately and the result arrives via
//! the rev-gated push frame. The iOS `summarize_episode` agent tool awaits the
//! projection until `episode.summary` populates.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::runtime::Runtime;

use crate::episode_summary_llm::summarize_episode;
use crate::store::PodcastStore;

/// Kick off a background summarization pass for one episode.
///
/// Returns immediately with `{"ok":true,"status":"summarizing"}` once the
/// inputs are gathered and the task is spawned, or an error envelope when the
/// episode is unknown or the store lock is poisoned.
pub(crate) fn handle_summarize_episode(
    store: &Arc<Mutex<PodcastStore>>,
    rev: &Arc<AtomicU64>,
    runtime: &Arc<Runtime>,
    episode_id: String,
) -> serde_json::Value {
    let inputs = match store.lock() {
        Ok(s) => match s.episode_summary_inputs(&episode_id) {
            Some(inputs) => inputs,
            None => {
                return serde_json::json!({
                    "ok": false,
                    "error": format!("episode not found: {episode_id}")
                })
            }
        },
        Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
    };

    let store_c = Arc::clone(store);
    let rev_c = Arc::clone(rev);
    let runtime_c = Arc::clone(runtime);

    runtime.spawn(async move {
        summarize_in_background(store_c, rev_c, runtime_c, episode_id, inputs).await;
    });

    serde_json::json!({"ok": true, "status": "summarizing"})
}

/// Background async summarization. Runs the LLM call off the actor thread via
/// `spawn_blocking`, then stamps the result and bumps `rev`. Failures are
/// logged and leave the episode's summary untouched.
async fn summarize_in_background(
    store: Arc<Mutex<PodcastStore>>,
    rev: Arc<AtomicU64>,
    runtime: Arc<Runtime>,
    episode_id: String,
    inputs: crate::store::summary::EpisodeSummaryInputs,
) {
    let runtime2 = Arc::clone(&runtime);
    let result = tokio::task::spawn_blocking(move || {
        summarize_episode(
            &inputs.title,
            &inputs.description,
            inputs.transcript.as_deref(),
            &runtime2,
        )
    })
    .await;

    match result {
        Ok(Ok(summary)) => {
            if let Ok(mut s) = store.lock() {
                if s.set_episode_summary(&episode_id, Some(summary)) {
                    rev.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
        Ok(Err(e)) => {
            eprintln!("[episode_summary] LLM summarize failed for {episode_id}: {e}");
        }
        Err(e) => {
            eprintln!("[episode_summary] spawn_blocking panicked for {episode_id}: {e}");
        }
    }
}

#[cfg(test)]
#[path = "episode_summary_tests.rs"]
mod tests;
