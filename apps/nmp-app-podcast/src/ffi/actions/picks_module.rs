//! Picks-action `ActionModule` — routes all `"podcast.picks.*"` dispatches.
//!
//! Feature #46 (AI agent picks). The rail fills in two stages: a fast
//! newest-first heuristic (newest episodes across the user's library, capped
//! per show for diversity, top-N overall) stamps the slot immediately, then a
//! background LLM pass (`picks_handler::handle_refresh` →
//! `picks_llm::score_episode_for_picks`) re-stamps it with a personalized
//! ranking conditioned on the user's listening profile. Both stages share this
//! action wire format; the LLM pass runs automatically after every feed refresh
//! (see `PodcastHostOpHandler::auto_refresh_picks`) and on explicit
//! `{"op":"refresh"}` dispatches.
//!
//! Swift encodes every picks action as `{"op":"<variant>"}`. The
//! `#[serde(tag = "op", rename_all = "snake_case")]` discriminator maps
//! the string `op` value to the enum variant. The module's `execute`
//! body forwards the whole action as `ActorCommand::DispatchHostOp` so
//! the `PodcastHostOpHandler` (running on the actor thread) can read the
//! `PodcastStore`, run the heuristic, and stamp the result onto the
//! shared `Arc<Mutex<Vec<AgentPickSummary>>>` slot on `PodcastHandle`.

use serde::{Deserialize, Serialize};

use nmp_core::substrate::ActionModule;
use nmp_core::actor::ActorCommand;

use crate::ffi::projections::AgentPickSummary;

/// Maximum picks the heuristic emits per refresh. The Home rail renders
/// a horizontal card stack — beyond ~10 the diversity-per-show cap stops
/// being meaningful and the rail just becomes a duplicate of the
/// "New Episodes" list below it.
pub const PICKS_LIMIT: usize = 10;

/// Maximum picks selected from any one show. Keeps the rail diverse:
/// without this, a single high-frequency feed (think a daily news show)
/// would monopolize all 10 slots.
pub const PICKS_PER_SHOW_CAP: usize = 2;

/// Wire enum for all `"podcast.picks"` namespace actions.
///
/// `#[serde(tag = "op", rename_all = "snake_case")]` makes the JSON
/// discriminator the lowercase snake-case variant name:
/// `refresh` → `{"op":"refresh"}`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PicksAction {
    /// Recompute the picks slot from the current library snapshot. The
    /// handler returns `{"ok":true}` once the slot is updated and
    /// bumps `rev` so the iOS poll observes the change.
    Refresh,
}

/// Action module for the `"podcast.picks"` namespace.
///
/// `execute` serializes the typed `PicksAction` back to JSON and hands it
/// to the actor as `ActorCommand::DispatchHostOp`. The installed
/// `PodcastHostOpHandler` deserializes it, runs the heuristic against the
/// store, writes the resulting `Vec<AgentPickSummary>` to the picks slot
/// on `PodcastHandle`, and returns a `{"ok":true}` envelope.
pub struct AgentPicksModule;

impl ActionModule for AgentPicksModule {
    const NAMESPACE: nmp_core::substrate::DeclaredActionNamespace =
        nmp_core::substrate::DeclaredActionNamespace::app_owned("podcast.picks");

    type Action = PicksAction;

    fn is_async_completing() -> bool {
        false
    }

    fn execute(
        &self,
        _ctx: &nmp_core::substrate::ActionContext,
        action: Self::Action,
        correlation_id: &str,
        send: &dyn Fn(ActorCommand),
    ) -> Result<(), String> {
        crate::ffi::actions::dispatch_host_op(Self::NAMESPACE.as_str(), &action, correlation_id, send)
    }

    fn decode_payload(
        bytes: &[u8],
    ) -> Option<Result<Self::Action, nmp_core::substrate::ActionPayloadDecodeError>> {
        crate::action_payload::decode_podcast_payload(bytes)
    }
}

// ---------------------------------------------------------------------------
// Heuristic picks computation — pure function for testability
// ---------------------------------------------------------------------------

/// One row consumed by [`compute_picks`]. The `host_op_handler` builds these
/// directly from the `PodcastStore` so the heuristic stays independent of the
/// store internals (and so we can drive tests without instantiating one).
#[derive(Clone, Debug, PartialEq)]
pub struct CandidateEpisode {
    pub episode_id: String,
    pub episode_title: String,
    pub podcast_id: String,
    pub podcast_title: String,
    pub artwork_url: Option<String>,
    pub published_at: i64,
    pub duration_secs: Option<f64>,
}

/// Compute the picks list from a flat candidate set.
///
/// Algorithm:
///   1. Sort candidates by `published_at` descending (newest first).
///   2. Walk in order, accepting each episode that has not yet exceeded
///      [`PICKS_PER_SHOW_CAP`] picks for its show.
///   3. Stop at [`PICKS_LIMIT`].
///   4. Assign `pick_score = 1.0 - (rank / PICKS_LIMIT as f32)` so the top
///      pick gets 1.0 and the last accepted pick gets a positive non-zero.
///   5. `pick_reason = "New from {podcast_title}"`.
pub fn compute_picks(mut candidates: Vec<CandidateEpisode>) -> Vec<AgentPickSummary> {
    // Newest first. `sort_by` is stable, so ties keep input order.
    candidates.sort_by(|a, b| b.published_at.cmp(&a.published_at));

    let mut per_show: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut picks: Vec<AgentPickSummary> = Vec::with_capacity(PICKS_LIMIT);

    for cand in candidates {
        if picks.len() >= PICKS_LIMIT {
            break;
        }
        let count = per_show.entry(cand.podcast_id.clone()).or_insert(0);
        if *count >= PICKS_PER_SHOW_CAP {
            continue;
        }
        *count += 1;

        let rank = picks.len();
        // rank 0 ⇒ 1.0, rank PICKS_LIMIT-1 ⇒ 1/PICKS_LIMIT.
        let score = 1.0_f32 - (rank as f32 / PICKS_LIMIT as f32);
        let reason = format!("New from {}", cand.podcast_title);

        picks.push(AgentPickSummary {
            episode_id: cand.episode_id,
            episode_title: cand.episode_title,
            podcast_id: cand.podcast_id,
            podcast_title: cand.podcast_title,
            artwork_url: cand.artwork_url,
            published_at: cand.published_at,
            duration_secs: cand.duration_secs,
            pick_reason: reason,
            pick_score: score,
        });
    }

    picks
}

/// Compute the picks list using LLM scores when available (M5.6).
///
/// Mirrors [`compute_picks`]'s diversity rules (per-show cap, total limit)
/// but the ranking signal is the LLM `(score, reason)` pair from
/// `scores` (keyed by `episode_id`). Candidates without an LLM score fall
/// back to a recency-normalized heuristic score so the rail still fills when
/// Ollama is partially offline.
///
/// Algorithm:
///   1. Resolve each candidate's `(score, reason)`: LLM if present in
///      `scores`, else `(recency_score, "New from {podcast_title}")`.
///   2. Sort by resolved score descending; ties broken newest-first so the
///      visible order is deterministic.
///   3. Walk in order, accepting each episode under [`PICKS_PER_SHOW_CAP`]
///      per show, stopping at [`PICKS_LIMIT`].
///   4. `pick_score` = resolved score; `pick_reason` = resolved reason.
pub fn compute_picks_scored(
    candidates: Vec<CandidateEpisode>,
    scores: &std::collections::HashMap<String, (f32, String)>,
) -> Vec<AgentPickSummary> {
    let now = chrono::Utc::now().timestamp();

    // Resolve (score, reason) per candidate up front so sorting is by the
    // final signal, not by recency.
    let mut resolved: Vec<(CandidateEpisode, f32, String)> = candidates
        .into_iter()
        .map(|cand| {
            let (score, reason) = match scores.get(&cand.episode_id) {
                Some((s, r)) => (*s, r.clone()),
                None => (
                    recency_score(now, cand.published_at),
                    format!("New from {}", cand.podcast_title),
                ),
            };
            (cand, score, reason)
        })
        .collect();

    // Highest score first; ties newest-first (deterministic).
    resolved.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.0.published_at.cmp(&a.0.published_at))
    });

    let mut per_show: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut picks: Vec<AgentPickSummary> = Vec::with_capacity(PICKS_LIMIT);

    for (cand, score, reason) in resolved {
        if picks.len() >= PICKS_LIMIT {
            break;
        }
        let count = per_show.entry(cand.podcast_id.clone()).or_insert(0);
        if *count >= PICKS_PER_SHOW_CAP {
            continue;
        }
        *count += 1;

        picks.push(AgentPickSummary {
            episode_id: cand.episode_id,
            episode_title: cand.episode_title,
            podcast_id: cand.podcast_id,
            podcast_title: cand.podcast_title,
            artwork_url: cand.artwork_url,
            published_at: cand.published_at,
            duration_secs: cand.duration_secs,
            pick_reason: reason,
            pick_score: score.clamp(0.0, 1.0),
        });
    }

    picks
}

/// Recency-normalized fallback score in `0.0..=1.0`, newest = highest.
///
/// Used by [`compute_picks_scored`] for candidates the LLM did not score.
/// Mirrors the inbox recency-bucket curve so unscored picks rank sensibly
/// against LLM-scored ones rather than collapsing to a constant.
fn recency_score(now_unix: i64, published_at_unix: i64) -> f32 {
    const ONE_HOUR: i64 = 3_600;
    const ONE_DAY: i64 = 24 * ONE_HOUR;
    const WINDOW_SECS: i64 = 30 * ONE_DAY;

    let age = (now_unix - published_at_unix).max(0);
    if age < 12 * ONE_HOUR {
        return 1.0;
    }
    if age < 3 * ONE_DAY {
        return 0.85;
    }
    if age < 7 * ONE_DAY {
        return 0.65;
    }
    if age < WINDOW_SECS {
        let progress = (age - 7 * ONE_DAY) as f32 / (WINDOW_SECS - 7 * ONE_DAY) as f32;
        return 0.55 - progress.clamp(0.0, 1.0) * 0.35;
    }
    0.15
}

#[cfg(test)]
#[path = "picks_module_tests.rs"]
mod tests;
