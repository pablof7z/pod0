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
//! ## File layout
//!
//! This file keeps the **pure projection** ([`build_inbox`] /
//! [`episodes_needing_triage`] / `score`). The action handlers and the
//! background LLM scoring pass live in the sibling `inbox_handler_triage.rs`
//! (re-exported below) so neither half exceeds the 500-line hard limit.
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
use std::sync::{Arc, Mutex};

use chrono::Utc;

use crate::ffi::projections::InboxItem;
use crate::inbox_llm::{TriageResult, TriageStatus};
use crate::store::PodcastStore;

// Triage **actions** + the background LLM scoring pass live in the sibling
// `inbox_handler_triage` module so neither half exceeds the 500-line hard
// limit. Re-exported here so external callers keep their
// `crate::inbox_handler::{handle_inbox_action, maybe_enqueue_triage, …}` paths,
// and so the in-module test submodules (which use `super::*`) still resolve
// the action handlers plus the `stamp_pending`/`reconcile_pending` helpers.
#[path = "inbox_handler_triage.rs"]
mod triage;
pub use triage::{
    handle_inbox_action, handle_inbox_action_with_signal, maybe_enqueue_triage,
    maybe_enqueue_triage_with_signal,
};
#[cfg(test)]
pub(crate) use triage::stamp_pending;

// The in-module test submodules below pull these names in via `use super::*`.
// They are only referenced from tests (the production projection in this file
// no longer touches the atomic/runtime/action types — those moved to the
// triage module), so the re-exports are test-gated to avoid unused-import
// warnings in the release build.
#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use {
    crate::ffi::actions::inbox_module::InboxAction,
    std::sync::atomic::{AtomicBool, AtomicU64, Ordering},
    tokio::runtime::Runtime,
};

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
                Some(tr) if tr.status == TriageStatus::Ready => (
                    tr.priority_score,
                    tr.priority_reason.clone(),
                    tr.categories.clone(),
                ),
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

/// Recency-weighted heuristic: score newer episodes higher.
///
/// Returns the score (`0.0..=1.0`) and a short human-readable bucket
/// label that the row caption renders verbatim. This is the fallback path
/// when no LLM triage result is available for an episode.
pub(crate) fn score(now_unix: i64, published_at_unix: i64) -> (f32, &'static str) {
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

#[cfg(test)]
#[path = "inbox_handler_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "inbox_proactive_tests.rs"]
mod proactive_tests;
