//! Player-action `ActionModule` — routes all `"podcast.player.*"` dispatches.
//!
//! Swift encodes every player action as `{"op":"<variant>", ...fields}`.
//! The `#[serde(tag = "op", rename_all = "snake_case")]` discriminator maps
//! the string `op` value to the enum variant. The module's `execute` body
//! forwards the whole action as `ActorCommand::DispatchHostOp` so the
//! `PodcastHostOpHandler` (running on the actor thread) can dispatch audio
//! capability commands without the kernel naming podcast-domain nouns (D0).

use serde::{Deserialize, Serialize};

use nmp_core::substrate::ActionModule;
use nmp_core::ActorCommand;

/// Wire enum for all `"podcast.player"` namespace actions.
///
/// `#[serde(tag = "op", rename_all = "snake_case")]` makes the JSON
/// discriminator the lowercase snake-case variant name:
/// `play` → `{"op":"play","episode_id":"..."}`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PlayerAction {
    Play { episode_id: String },
    Pause,
    Seek { position_secs: f64 },
    SetSpeed { speed: f32 },
    SetVolume { volume: f32 },
    SetSleepTimer {
        #[serde(default)]
        secs: Option<u64>,
    },
    Stop,
}

/// Action module for the `"podcast.player"` namespace.
///
/// `execute` serializes the typed `PlayerAction` back to JSON and hands it
/// to the actor as `ActorCommand::DispatchHostOp`. The installed
/// `PodcastHostOpHandler` deserializes it, dispatches the matching
/// `AudioCommand` to the audio capability, and returns a `{"ok":true}` envelope.
pub struct PlayerActionModule;

impl ActionModule for PlayerActionModule {
    const NAMESPACE: &'static str = "podcast.player";

    type Action = PlayerAction;

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
    fn play_action_round_trips() {
        let action = PlayerAction::Play {
            episode_id: "abc-123".into(),
        };
        let json = serde_json::to_string(&action).expect("encode");
        assert!(json.contains(r#""op":"play""#));
        assert!(json.contains(r#""episode_id":"abc-123""#));
        let decoded: PlayerAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, action);
    }

    #[test]
    fn pause_stop_are_unit_variants() {
        for (action, expected_op) in [
            (PlayerAction::Pause, "pause"),
            (PlayerAction::Stop, "stop"),
        ] {
            let json = serde_json::to_string(&action).expect("encode");
            assert!(json.contains(&format!(r#""op":"{expected_op}""#)));
            let decoded: PlayerAction = serde_json::from_str(&json).expect("decode");
            assert_eq!(decoded, action);
        }
    }

    #[test]
    fn seek_encodes_position() {
        let action = PlayerAction::Seek { position_secs: 42.5 };
        let json = serde_json::to_string(&action).expect("encode");
        assert_eq!(json, r#"{"op":"seek","position_secs":42.5}"#);
        let decoded: PlayerAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, action);
    }

    #[test]
    fn set_sleep_timer_handles_some_and_none() {
        let arm = PlayerAction::SetSleepTimer { secs: Some(1800) };
        let json = serde_json::to_string(&arm).expect("encode");
        assert!(json.contains("1800"));
        let decoded: PlayerAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, arm);

        let cancel = PlayerAction::SetSleepTimer { secs: None };
        let cancel_json = serde_json::to_string(&cancel).expect("encode");
        let decoded_cancel: PlayerAction = serde_json::from_str(&cancel_json).expect("decode");
        assert_eq!(decoded_cancel, cancel);
    }

    #[test]
    fn execute_emits_dispatch_host_op() {
        let action = PlayerAction::Play {
            episode_id: "ep-7".into(),
        };
        let commands = std::sync::Mutex::new(Vec::<ActorCommand>::new());
        PlayerActionModule::execute(action, "corr-1", &|cmd| {
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
        assert_eq!(v["op"], "play");
        assert_eq!(v["episode_id"], "ep-7");
    }
}
