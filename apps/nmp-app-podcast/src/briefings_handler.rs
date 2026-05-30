//! Briefing-specific host op handlers.
//!
//! Split out of [`crate::host_op_handler`] so the M9 briefing surface
//! (composer dispatch, scheduler updates, briefing snapshot building)
//! has a dedicated home as it grows in M9.B. M9.A only needs the
//! generate-briefing stub; subsequent milestones land here.
//!
//! Per D0 / D6: every entry point returns an in-band JSON envelope so
//! callers can dispatch without a back-channel for failures.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use tokio::runtime::Runtime;

use crate::briefing_llm;
use crate::ffi::projections::{BriefingSegmentSummary, BriefingSnapshot};
use crate::store::PodcastStore;

/// Max number of recent unplayed episodes folded into the briefing prompt.
const MAX_BRIEFING_EPISODES: usize = 10;

/// Handler for the `podcast.generate_briefing` action (M5.6).
///
/// Synchronously flips the snapshot's briefing slot into the `"generating"`
/// lifecycle state so the iOS Briefings tab immediately renders the
/// "Composing your briefing" placeholder and stops showing the empty-state
/// CTA on the next snapshot poll, then returns the optimistic
/// `{"ok": true, "status": "generating"}` envelope and bumps `rev`.
///
/// When a `store` + `runtime` are wired (production path) it then spawns a
/// background task that:
/// 1. collects the top [`MAX_BRIEFING_EPISODES`] most-recent unplayed
///    episodes across all subscriptions,
/// 2. asks Ollama (via [`briefing_llm::generate_briefing_segments`]) for a
///    3–5 item briefing,
/// 3. writes the resulting segments into the slot, flips `status = "ready"`
///    / `is_generating = false`, stamps `last_generated_at`, and bumps `rev`.
///
/// If Ollama is offline (or the reply can't be parsed) the task falls back to
/// [`briefing_llm::fallback_segments`] — a no-LLM one-segment summary built
/// from episode titles — so the briefing always leaves the `generating`
/// state. In unit tests (no `store`/`runtime`) the slot stays in the
/// `generating` state, matching the prior stub behaviour.
///
/// Per D6 the response is always an in-band envelope; the iOS view surfaces
/// `status` as the optimistic UI hint.
pub fn handle_generate_briefing(
    slot: &Arc<Mutex<Option<BriefingSnapshot>>>,
    rev: &Arc<AtomicU64>,
    store: Option<&Arc<Mutex<PodcastStore>>>,
    runtime: Option<&Arc<Runtime>>,
) -> serde_json::Value {
    if let Ok(mut s) = slot.lock() {
        *s = Some(BriefingSnapshot {
            status: "generating".to_owned(),
            is_generating: true,
            segment_count: 0,
            segments: Vec::new(),
            last_generated_at: None,
            next_scheduled_minutes: None,
        });
        rev.fetch_add(1, Ordering::Relaxed);
    }

    if let (Some(store), Some(runtime)) = (store, runtime) {
        // Collect recent unplayed episodes under the store lock, then release
        // it *before* spawning so the actor thread never blocks on the LLM
        // round-trip (lock discipline mirrors `wiki::handle_generate`).
        let episodes = collect_recent_unplayed(store);

        let slot_c = Arc::clone(slot);
        let rev_c = Arc::clone(rev);
        let runtime_c = Arc::clone(runtime);

        runtime.spawn(async move {
            let segments = tokio::task::spawn_blocking(move || {
                briefing_llm::generate_briefing_segments(&episodes, &runtime_c)
                    .unwrap_or_else(|_| briefing_llm::fallback_segments(&episodes))
            })
            .await
            .unwrap_or_else(|_| {
                vec!["Could not compose a briefing right now. Please try again.".to_owned()]
            });

            if let Ok(mut s) = slot_c.lock() {
                let summaries: Vec<BriefingSegmentSummary> = segments
                    .into_iter()
                    .map(|text| BriefingSegmentSummary {
                        kind: "episode_summary".to_owned(),
                        text,
                        podcast_title: None,
                        episode_title: None,
                    })
                    .collect();
                *s = Some(BriefingSnapshot {
                    status: "ready".to_owned(),
                    is_generating: false,
                    segment_count: summaries.len(),
                    segments: summaries,
                    last_generated_at: Some(Utc::now().timestamp()),
                    next_scheduled_minutes: None,
                });
            }
            rev_c.fetch_add(1, Ordering::Relaxed);
        });
    }

    serde_json::json!({"ok": true, "status": "generating"})
}

/// Walk all subscriptions and collect the top [`MAX_BRIEFING_EPISODES`]
/// most-recent unplayed episodes as `(podcast_title, episode_title,
/// description)`, newest first. Holds the store lock for the duration of the
/// walk only; the caller must not hold it when calling.
fn collect_recent_unplayed(
    store: &Arc<Mutex<PodcastStore>>,
) -> Vec<(String, String, String)> {
    let guard = match store.lock() {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    // (pub_date_ts, podcast_title, episode_title, description)
    let mut rows: Vec<(i64, String, String, String)> = Vec::new();
    for (podcast, episodes) in guard.all_podcasts() {
        for ep in episodes {
            if ep.played {
                continue;
            }
            rows.push((
                ep.pub_date.timestamp(),
                podcast.title.clone(),
                ep.title.clone(),
                ep.description.clone(),
            ));
        }
    }
    // Newest first; truncate to the prompt budget.
    rows.sort_by(|a, b| b.0.cmp(&a.0));
    rows.truncate(MAX_BRIEFING_EPISODES);
    rows.into_iter()
        .map(|(_, podcast_title, ep_title, desc)| (podcast_title, ep_title, desc))
        .collect()
}

#[cfg(test)]
#[path = "briefings_handler_tests.rs"]
mod tests;
