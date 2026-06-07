//! Agent-scheduled-tasks ActionModule — routes all `"podcast.tasks.*"` dispatches.
//!
//! Swift encodes every task action as `{"op":"<variant>", ...fields}`.
//! The `#[serde(tag = "op", rename_all = "snake_case")]` discriminator maps
//! the string `op` value to the enum variant. The module's `execute` body
//! forwards the whole action as `ActorCommand::DispatchHostOp` so the
//! `PodcastHostOpHandler` (running on the actor thread) can mutate the
//! `agent_tasks` slot on the `PodcastHandle` without the kernel naming
//! podcast-domain nouns (D0).
//!
//! See [`crate::ffi::projections::AgentTaskSummary`] for the projection
//! shape that backs each entry, and `tasks_handler.rs` for the host-op
//! routing logic.

use serde::{Deserialize, Serialize};

use nmp_core::substrate::ActionModule;
use nmp_core::ActorCommand;

/// Wire enum for all `"podcast.tasks"` namespace actions.
///
/// `#[serde(tag = "op", rename_all = "snake_case")]` makes the JSON
/// discriminator the lowercase snake-case variant name:
/// `create` → `{"op":"create","title":"…","schedule":"daily", …}`.
///
/// `create_from_intent` mints a UUID server-side (Rust-side), resolves the
/// typed task intent to the internal dispatch payload, and returns
/// `{"ok":true,"task_id":"<uuid>"}`. `create` remains for compatibility with
/// older clients that already send raw dispatch namespace/body pairs; new
/// clients should use typed intents.
///
/// `run_now` is a stub: it marks the task `completed` + stamps
/// `last_run_at` rather than actually dispatching the `action_namespace`
/// payload. The real receiver action (`podcast.inbox.triage`) is wired
/// through the task dispatch hook; the stub path remains for unit tests
/// with no live kernel and preserves the wire shape.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum AgentTasksAction {
    /// Compatibility/internal path for older clients that already know the
    /// backend dispatch namespace/body contract. User-facing clients should
    /// prefer [`AgentTasksAction::CreateFromIntent`].
    Create {
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        action_namespace: String,
        action_body: String,
        schedule: String,
    },
    CreateFromIntent {
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        intent: AgentTaskIntent,
        schedule: String,
    },
    Delete {
        task_id: String,
    },
    Enable {
        task_id: String,
    },
    Disable {
        task_id: String,
    },
    RunNow {
        task_id: String,
    },
}

/// Typed task intents accepted by `"podcast.tasks"` creation.
///
/// The handler resolves these stable user-level intents to the internal
/// `(action_namespace, action_body)` pair used by `run_now`. This keeps clients
/// from constructing raw action JSON while preserving the current scheduler
/// storage shape.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentTaskIntent {
    InboxTriage,
    ClearAgent,
    RememberMemory { key: String, value: String },
}

/// Action module for the `"podcast.tasks"` namespace.
///
/// `execute` serializes the typed `AgentTasksAction` back to JSON and
/// hands it to the actor as `ActorCommand::DispatchHostOp`. The
/// installed `PodcastHostOpHandler` deserializes it, mutates the
/// `agent_tasks` slot, bumps `rev` so the next snapshot poll picks up
/// the change, and returns a `{"ok":true}` envelope. All policy lives
/// in the handler; the action module is pure routing — matching the
/// pattern established by `PodcastActionModule` / `PlayerActionModule`.
pub struct AgentTasksModule;

impl ActionModule for AgentTasksModule {
    const NAMESPACE: &'static str = "podcast.tasks";

    type Action = AgentTasksAction;

    fn is_async_completing() -> bool {
        false
    }

    fn execute(
        action: Self::Action,
        correlation_id: &str,
        send: &dyn Fn(ActorCommand),
    ) -> Result<(), String> {
        let action_json = serde_json::to_string(&action).map_err(|e| e.to_string())?;
        send(ActorCommand::DispatchHostOp {
            action_json,
            correlation_id: correlation_id.to_owned(),
        });
        Ok(())
    }
}

#[cfg(test)]
#[path = "tasks_module_tests.rs"]
mod tests;
