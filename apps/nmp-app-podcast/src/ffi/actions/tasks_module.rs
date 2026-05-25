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
/// `create` mints a UUID server-side (Rust-side) and returns
/// `{"ok":true,"task_id":"<uuid>"}`. The other ops take the existing
/// `task_id` and return `{"ok":true}` (or `{"ok":false,"error":...}` on
/// unknown id).
///
/// `run_now` is a stub: it marks the task `completed` + stamps
/// `last_run_at` rather than actually dispatching the `action_namespace`
/// payload. The real receiver actions (`podcast.briefings.generate`,
/// `podcast.inbox.triage`) don't exist yet as ActionModules; once they
/// land the stub will be swapped for a real dispatch without changing
/// the wire shape.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum AgentTasksAction {
    Create {
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        action_namespace: String,
        action_body: String,
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
mod tests {
    use super::*;

    #[test]
    fn create_action_round_trips_with_all_fields() {
        let action = AgentTasksAction::Create {
            title: "Morning Briefing".into(),
            description: Some("Daily briefing".into()),
            action_namespace: "podcast.briefings.generate".into(),
            action_body: "{}".into(),
            schedule: "daily".into(),
        };
        let json = serde_json::to_string(&action).expect("encode");
        assert!(json.contains(r#""op":"create""#));
        assert!(json.contains(r#""title":"Morning Briefing""#));
        assert!(json.contains(r#""action_namespace":"podcast.briefings.generate""#));
        let decoded: AgentTasksAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, action);
    }

    #[test]
    fn create_action_omits_none_description() {
        let action = AgentTasksAction::Create {
            title: "Inbox Triage".into(),
            description: None,
            action_namespace: "podcast.inbox.triage".into(),
            action_body: "{}".into(),
            schedule: "daily".into(),
        };
        let json = serde_json::to_string(&action).expect("encode");
        assert!(!json.contains("description"));
        let decoded: AgentTasksAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, action);
    }

    #[test]
    fn delete_action_round_trips() {
        let action = AgentTasksAction::Delete {
            task_id: "task-1".into(),
        };
        let json = serde_json::to_string(&action).expect("encode");
        assert!(json.contains(r#""op":"delete""#));
        assert!(json.contains(r#""task_id":"task-1""#));
        let decoded: AgentTasksAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, action);
    }

    #[test]
    fn enable_disable_actions_round_trip() {
        for (action, expected_op) in [
            (
                AgentTasksAction::Enable {
                    task_id: "task-1".into(),
                },
                "enable",
            ),
            (
                AgentTasksAction::Disable {
                    task_id: "task-1".into(),
                },
                "disable",
            ),
        ] {
            let json = serde_json::to_string(&action).expect("encode");
            assert!(json.contains(&format!(r#""op":"{expected_op}""#)));
            let decoded: AgentTasksAction = serde_json::from_str(&json).expect("decode");
            assert_eq!(decoded, action);
        }
    }

    #[test]
    fn run_now_action_round_trips() {
        let action = AgentTasksAction::RunNow {
            task_id: "task-1".into(),
        };
        let json = serde_json::to_string(&action).expect("encode");
        assert!(json.contains(r#""op":"run_now""#));
        let decoded: AgentTasksAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, action);
    }

    #[test]
    fn execute_emits_dispatch_host_op() {
        let action = AgentTasksAction::Delete {
            task_id: "task-1".into(),
        };
        let commands = std::sync::Mutex::new(Vec::<ActorCommand>::new());
        AgentTasksModule::execute(action, "corr-1", &|cmd| {
            commands.lock().unwrap().push(cmd);
        })
        .expect("execute ok");
        let commands = commands.into_inner().unwrap();
        assert_eq!(commands.len(), 1);
        let ActorCommand::DispatchHostOp {
            action_json,
            correlation_id,
        } = &commands[0]
        else {
            panic!("expected DispatchHostOp");
        };
        assert_eq!(correlation_id, "corr-1");
        let v: serde_json::Value = serde_json::from_str(action_json).expect("json");
        assert_eq!(v["op"], "delete");
        assert_eq!(v["task_id"], "task-1");
    }
}
