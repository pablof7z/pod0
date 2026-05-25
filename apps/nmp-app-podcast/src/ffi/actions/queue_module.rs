//! Playback-queue `ActionModule` — routes all `"podcast.queue.*"` dispatches.
//!
//! Swift encodes every queue action as `{"op":"<variant>", ...fields}`.
//! The `#[serde(tag = "op", rename_all = "snake_case")]` discriminator maps
//! the string `op` value to the enum variant. The module's `execute` body
//! forwards the whole action as `ActorCommand::DispatchHostOp` so the
//! `PodcastHostOpHandler` (running on the actor thread) can mutate the
//! shared [`crate::queue::PlaybackQueue`] without the kernel naming
//! podcast-domain nouns (D0).
//!
//! ## Wire shape
//!
//! ```text
//! podcast.queue.add_next  { episode_id }   — push to the front
//! podcast.queue.add_last  { episode_id }   — push to the back
//! podcast.queue.remove    { episode_id }   — drop from anywhere
//! podcast.queue.clear     { }              — empty the queue
//! ```
//!
//! Every variant returns the canonical `{"ok": true}` envelope.

use serde::{Deserialize, Serialize};

use nmp_core::substrate::ActionModule;
use nmp_core::ActorCommand;

/// Wire enum for all `"podcast.queue"` namespace actions.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum QueueAction {
    /// Push the episode onto the front of the queue ("Play Next").
    AddNext { episode_id: String },
    /// Push the episode onto the back of the queue ("Add to Queue").
    AddLast { episode_id: String },
    /// Drop the episode from anywhere in the queue.
    Remove { episode_id: String },
    /// Empty the queue.
    Clear,
}

/// Action module for the `"podcast.queue"` namespace.
///
/// `execute` serializes the typed [`QueueAction`] back to JSON and hands it
/// to the actor as [`ActorCommand::DispatchHostOp`]. The installed
/// [`crate::host_op_handler::PodcastHostOpHandler`] deserializes it, mutates
/// the [`crate::queue::PlaybackQueue`], bumps `rev`, and returns
/// `{"ok": true}`.
pub struct QueueActionModule;

impl ActionModule for QueueActionModule {
    const NAMESPACE: &'static str = "podcast.queue";

    type Action = QueueAction;

    fn is_async_completing() -> bool {
        false
    }

    fn execute(
        action: Self::Action,
        correlation_id: &str,
        send: &dyn Fn(ActorCommand),
    ) -> Result<(), String> {
        let action_json =
            serde_json::to_string(&action).map_err(|e| e.to_string())?;
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
    fn add_next_action_round_trips() {
        let action = QueueAction::AddNext {
            episode_id: "ep-7".into(),
        };
        let json = serde_json::to_string(&action).expect("encode");
        assert!(json.contains(r#""op":"add_next""#));
        assert!(json.contains(r#""episode_id":"ep-7""#));
        let decoded: QueueAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, action);
    }

    #[test]
    fn add_last_action_round_trips() {
        let action = QueueAction::AddLast {
            episode_id: "ep-7".into(),
        };
        let json = serde_json::to_string(&action).expect("encode");
        assert!(json.contains(r#""op":"add_last""#));
        assert!(json.contains(r#""episode_id":"ep-7""#));
        let decoded: QueueAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, action);
    }

    #[test]
    fn remove_action_round_trips() {
        let action = QueueAction::Remove {
            episode_id: "ep-7".into(),
        };
        let json = serde_json::to_string(&action).expect("encode");
        assert!(json.contains(r#""op":"remove""#));
        assert!(json.contains(r#""episode_id":"ep-7""#));
        let decoded: QueueAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, action);
    }

    #[test]
    fn clear_action_round_trips() {
        let action = QueueAction::Clear;
        let json = serde_json::to_string(&action).expect("encode");
        assert_eq!(json, r#"{"op":"clear"}"#);
        let decoded: QueueAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, action);
    }

    #[test]
    fn execute_emits_dispatch_host_op() {
        let action = QueueAction::AddNext {
            episode_id: "ep-7".into(),
        };
        let commands = std::sync::Mutex::new(Vec::<ActorCommand>::new());
        QueueActionModule::execute(action, "corr-1", &|cmd| {
            commands.lock().unwrap().push(cmd);
        })
        .expect("execute ok");
        let commands = commands.into_inner().unwrap();
        assert_eq!(commands.len(), 1);
        let ActorCommand::DispatchHostOp { action_json, correlation_id } = &commands[0] else {
            panic!("expected DispatchHostOp");
        };
        assert_eq!(correlation_id, "corr-1");
        let v: serde_json::Value = serde_json::from_str(action_json).expect("json");
        assert_eq!(v["op"], "add_next");
        assert_eq!(v["episode_id"], "ep-7");
    }

    #[test]
    fn namespace_is_podcast_queue() {
        assert_eq!(QueueActionModule::NAMESPACE, "podcast.queue");
    }
}
