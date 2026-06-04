//! Inbox triage cache types.
//!
//! [`TriageResult`] and [`TriageStatus`] are the canonical types for the
//! in-memory triage cache. Serializable so the cache can be persisted across
//! app launches (`store::inbox_triage_cache`).
//!
//! The LLM scoring logic that populates the cache lives in
//! `inbox_handler::triage_episodes_in_background`, which drives the agent via
//! `agent_llm::run_background_agent_task` + `agent_tools::ToolRegistry::for_triage`.

use serde::{Deserialize, Serialize};

/// Lifecycle status of a cached triage entry.
///
/// `Serialize`/`Deserialize` so the whole [`TriageResult`] can be persisted to
/// `<data_dir>/inbox-triage-cache.json` and reloaded on a cold launch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriageStatus {
    /// The agent produced a usable score; `priority_score` / `priority_reason` /
    /// `categories` are authoritative and `build_inbox` uses them verbatim.
    Ready,
    /// The agent call failed or was skipped. The score fields are placeholders
    /// and `build_inbox` ignores them in favor of the recency heuristic. The
    /// entry exists only to record `attempted_at` so the proactive trigger
    /// applies a retry cooldown instead of re-spawning every snapshot tick.
    Pending,
}

/// Result of agent-based episode triage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriageResult {
    /// Normalized priority score in the range `0.0..=1.0`.
    pub priority_score: f32,
    /// One-sentence human-readable reason for the score.
    pub priority_reason: String,
    /// Zero or more topic / guest category labels.
    pub categories: Vec<String>,
    /// Whether this entry carries a real agent score (`Ready`) or is a
    /// failure placeholder awaiting retry (`Pending`).
    pub status: TriageStatus,
    /// Unix seconds when the triage attempt that produced this entry ran.
    /// Drives both 24h staleness (for `Ready`) and the retry cooldown
    /// (for `Pending`) in `inbox_handler::episodes_needing_triage`.
    pub attempted_at: i64,
}

impl TriageResult {
    /// Construct a successful (`Ready`) triage entry stamped at `attempted_at`.
    pub fn ready(
        priority_score: f32,
        priority_reason: String,
        categories: Vec<String>,
        attempted_at: i64,
    ) -> Self {
        Self {
            priority_score,
            priority_reason,
            categories,
            status: TriageStatus::Ready,
            attempted_at,
        }
    }

    /// Construct a failure placeholder (`Pending`) stamped at `attempted_at`.
    /// The score fields are inert; `build_inbox` falls back to the heuristic
    /// for `Pending` entries.
    pub fn pending(attempted_at: i64) -> Self {
        Self {
            priority_score: 0.0,
            priority_reason: String::new(),
            categories: Vec::new(),
            status: TriageStatus::Pending,
            attempted_at,
        }
    }
}
