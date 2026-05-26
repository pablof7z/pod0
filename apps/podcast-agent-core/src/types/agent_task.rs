//! Agent task domain.
//!
//! An [`AgentTask`] is one unit of work the agent has been asked to do —
//! transcribe an episode, summarize one, run a search, compose a briefing.
//! Tasks are persisted in the [`crate::projections::ConversationActor`]
//! sibling stores (M7.B wires that store; M7.A defines the shape).
//!
//! The minimal scaffolding here is intentional: M7.B grows the runner,
//! the dispatcher, and the budget caps; M7.A only fixes the wire shape so
//! Swift `Codable` decoders and SQLite migrations have a stable target.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// What kind of work this task represents.
///
/// Each variant carries the inputs the runner needs. Ids stay as `String`
/// in M7.A so the snapshot decoder doesn't need any cross-crate
/// dependency wiring beyond `podcast-core`; M7.B can tighten to
/// `EpisodeId` once the dispatcher imports the typed alias.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskKind {
    Transcribe { episode_id: String },
    Summarize { episode_id: String },
    Search { query: String },
    Compose { topic: String },
}

/// Current state of an [`AgentTask`].
///
/// `Failed` carries the diagnostic string verbatim — surfaced into the
/// run log + conversation transcript so the user sees what went wrong
/// without spelunking through capability logs.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed { error: String },
}

impl TaskStatus {
    /// Whether the task is in a terminal state (`Completed` / `Failed`).
    ///
    /// Used by future M7.B logic to decide if a task can be retried or
    /// must be re-spawned. Defined here so consumers don't open-code the
    /// match.
    pub fn is_terminal(&self) -> bool {
        matches!(self, TaskStatus::Completed | TaskStatus::Failed { .. })
    }
}

/// One row in the agent's task ledger.
///
/// Newly-minted tasks land in `TaskStatus::Pending`; the runner moves
/// them through `Running` → terminal. The wall-clock `created_at` is the
/// scheduler's tie-breaker for FIFO dispatch.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct AgentTask {
    pub id: Uuid,
    pub kind: TaskKind,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
}

impl AgentTask {
    /// Mint a fresh `Pending` task for `kind`, stamping `created_at` to
    /// the wall clock. Tests that need determinism build the struct
    /// literally.
    pub fn new(kind: TaskKind) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind,
            status: TaskStatus::Pending,
            created_at: Utc::now(),
        }
    }
}

#[cfg(test)]
#[path = "agent_task_tests.rs"]
mod tests;
