use serde::{Deserialize, Serialize};

/// Snapshot of the agent-chat projection surfaced via
/// [`super::snapshot::PodcastUpdate::agent`].
///
/// Built by the future M7.B action-module wiring from a
/// [`podcast_agent_core::ConversationActor`]. Kept narrow on purpose:
/// the UI needs the active count + the pending-approvals queue + the
/// id of the most recently touched conversation; the rest of the
/// conversation rows are paged in by separate accessors.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct ConversationsSnapshot {
    /// Number of conversations the actor is currently tracking.
    pub active_count: usize,
    /// Outstanding approvals the user still has to decide on.
    ///
    /// Sorted oldest-first by the projection layer
    /// (`podcast_agent_core::sorted_active_approvals`) so the UI can
    /// render the queue without re-sorting.
    pub pending_approvals: Vec<PendingApprovalSnapshot>,
    /// Most recently touched conversation id (UUID rendered as the
    /// canonical hyphenated string), or `None` when the actor is
    /// empty. Surfaced as `String` rather than typed `Uuid` so the
    /// iOS shell's Codable decoder can treat it as a plain id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_conversation_id: Option<String>,
}

/// One row in [`ConversationsSnapshot::pending_approvals`].
///
/// `requested_at` is surfaced as a Unix timestamp (seconds since
/// epoch) rather than ISO-8601 so the iOS view model can compare
/// against `Date()` without a formatter round-trip — matches the
/// pattern the legacy `NostrPendingApproval` view code already uses.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct PendingApprovalSnapshot {
    pub id: String,
    pub description: String,
    /// Unix seconds — see struct-level comment.
    pub requested_at: i64,
}

/// One message in the agent-chat transcript surfaced via
/// [`AgentSnapshot::messages`].
///
/// `role` is a string (`"user"` / `"assistant"`) rather than an enum because
/// the iOS Swift `Codable` decoder switches on it directly without needing
/// a domain enum on the wire. `is_generating` lets the UI render an in-place
/// typing indicator on an assistant placeholder bubble while the kernel is
/// still composing the response — once the real LLM integration lands, that
/// flag flips back to `false` when the final content is filled in.
///
/// `created_at` is Unix seconds (epoch) so SwiftUI views can format
/// against `Date()` without a string round-trip — matches the pattern
/// used by [`PendingApprovalSnapshot::requested_at`].
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct AgentMessageSummary {
    pub id: String,
    /// `"user"` for messages the user sent, `"assistant"` for replies the
    /// agent produced.
    pub role: String,
    pub content: String,
    /// Unix seconds — see struct-level comment.
    pub created_at: i64,
    /// `true` while the assistant is still composing this message
    /// (placeholder bubble with the typing indicator).
    pub is_generating: bool,
}

/// Agent-chat conversation surfaced via
/// [`super::snapshot::PodcastUpdate::agent`].
///
/// Holds the full ordered transcript of the active conversation plus an
/// `is_busy` flag the UI uses to disable the send button + render the
/// typing indicator. The conversation lives on the
/// [`super::handle::PodcastHandle`] for the lifetime of the kernel;
/// clearing it is a dedicated `podcast.agent.clear` action so the UI
/// doesn't have to manage history state on the Swift side.
///
/// This is intentionally narrower than [`ConversationsSnapshot`] (which is
/// reserved for the future multi-conversation surface backed by
/// `podcast_agent_core::ConversationActor`): the feature-32 UI scaffold
/// only needs a single linear thread, and a wider shape would invite the
/// iOS view to depend on policy the kernel doesn't yet enforce.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct AgentSnapshot {
    /// Ordered transcript — oldest message first. The UI renders this
    /// directly without re-sorting.
    pub messages: Vec<AgentMessageSummary>,
    /// `true` while the kernel is producing a response. UI uses this to
    /// disable the send button and render the typing indicator. Always
    /// `false` in the scaffold (the canned reply is committed
    /// synchronously); the field is on the wire now so real LLM
    /// integration can flip it without a schema bump.
    pub is_busy: bool,
}

/// One agent-curated pick row surfaced via
/// [`super::snapshot::PodcastUpdate::picks`].
///
/// Built by the picks-projection layer (see `picks_module::AgentPicksModule`)
/// from a heuristic walk over the library: newest episodes across all
/// subscribed shows, capped per show for diversity, top-N overall. Real
/// LLM-driven picks land in a follow-up; the wire shape is forward-
/// compatible — the `pick_score` + `pick_reason` fields are populated by
/// whichever projection is in effect.
///
/// The fields are pre-resolved (podcast title + artwork denormalized from
/// `PodcastSummary`) so the iOS Home view can render the pick rail
/// without a second snapshot lookup per row.
///
/// `pick_score` is a `0.0..=1.0` confidence (1.0 = best pick); the
/// projection layer normalizes whichever signal it uses (recency rank in
/// the heuristic stub, model probability in the future LLM variant) onto
/// this range. `pick_reason` is a short, human-readable string the UI
/// renders directly (no localization in M-stub).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct AgentPickSummary {
    /// Stable id of the episode the pick refers to. Matches
    /// `EpisodeSummary.id`.
    pub episode_id: String,
    pub episode_title: String,
    /// Owning podcast's id (string form of `PodcastId` UUID).
    pub podcast_id: String,
    pub podcast_title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artwork_url: Option<String>,
    /// Unix seconds.
    pub published_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<f64>,
    /// Short reason the row was selected, rendered in the chip overlay.
    /// e.g. `"New from {podcast_title}"`.
    pub pick_reason: String,
    /// `0.0..=1.0` — higher is better. Used for sort order; the
    /// projection layer is the sole owner of normalization.
    pub pick_score: f32,
}

/// One row in the agent-tasks projection surfaced via
/// [`super::snapshot::PodcastUpdate::agent_tasks`].
///
/// Mirrors a recurring or one-shot action the agent has scheduled on
/// the user's behalf (e.g. "fetch new episodes every morning",
/// "triage the inbox at 7am", "research topic X").
///
/// Per D5/D7 this is pure data: the projection is consumed by the
/// `AgentTasksView` SwiftUI list and rendered without any client-side
/// logic. The kernel-side `AgentTasksModule` owns mutation; the iOS
/// shell only dispatches actions and re-renders.
///
/// `action_namespace` + `action_body` carry the dispatch payload the
/// task should fire (e.g. `"podcast.inbox.triage"` + `"{}"`).
/// Carrying them as opaque string fields keeps the projection
/// open-ended — new agent capabilities show up as new namespace
/// strings without changing this struct.
///
/// `schedule` is a free-form string (`"daily"`, `"weekly"`, `"once"`,
/// or a cron-like expression) — interpreted by the future scheduler
/// runtime, not by the projection layer.
///
/// `next_run_at` / `last_run_at` are surfaced as optional Unix
/// timestamps (seconds since epoch). `None` means the slot is
/// undefined (not-yet-scheduled / not-yet-run). The iOS view uses
/// these to render a relative-time chip.
///
/// `status` is one of `"pending"` / `"running"` / `"completed"` /
/// `"failed"`; carried as a string for the same string-discriminator
/// reason as [`DownloadItemSnapshot::state`].
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct AgentTaskSummary {
    /// Stable UUID (hyphenated string) minted by `AgentTasksModule::create`.
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// e.g. `"podcast.inbox.triage"` — the action namespace
    /// `run_now` would (in a future scheduler) dispatch.
    pub action_namespace: String,
    /// JSON payload (already-encoded). Keeps the projection
    /// schema-agnostic to the receiver's action shape.
    pub action_body: String,
    /// Free-form schedule label (`"daily"`, `"weekly"`, `"once"`, or
    /// a cron-like expression). The future scheduler runtime parses
    /// this; the projection layer treats it as opaque.
    pub schedule: String,
    /// Unix seconds — next scheduled run. `None` until the scheduler
    /// computes one or for `"once"` schedules that have already run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_run_at: Option<i64>,
    /// Unix seconds — last completed (or failed) run. `None` until
    /// the task has run at least once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_run_at: Option<i64>,
    /// Lifecycle label: `"pending"` / `"running"` / `"completed"` /
    /// `"failed"`. Defaults to `"pending"` on `create`.
    pub status: String,
    /// `true` when the scheduler should consider this task; user can
    /// toggle via `enable` / `disable` ops without losing the row.
    pub is_enabled: bool,
}
