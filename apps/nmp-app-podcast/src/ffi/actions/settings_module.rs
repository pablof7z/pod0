//! Settings `ActionModule` — routes `"podcast.settings.*"` dispatches.
//!
//! Swift encodes every settings action as `{"op":"<variant>", ...fields}`.
//! The `#[serde(tag = "op", rename_all = "snake_case")]` discriminator
//! maps the string `op` value to the enum variant. The module's
//! `execute` body forwards the whole action as
//! `ActorCommand::DispatchHostOp` so the `PodcastHostOpHandler`
//! (running on the actor thread) can mutate `PodcastStore` settings +
//! mirror the changed value into `PlayerActor` where relevant.

use serde::{Deserialize, Serialize};

use nmp_core::substrate::ActionModule;
use nmp_core::ActorCommand;

/// Wire enum for all `"podcast.settings"` namespace actions.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum SettingsAction {
    /// Set the user-facing auto-skip-ads toggle. The store persists
    /// the flag; the active `PlayerActor` is updated in lock-step so
    /// the next `Playing` report sees the new value without waiting
    /// for a `play` action.
    SetAutoSkipAds { enabled: bool },
    /// Update both skip intervals. Clamped server-side to `[1, 120]` seconds.
    /// iOS dispatches this when the user changes the skip interval in Settings.
    SetSkipIntervals { forward_secs: f64, backward_secs: f64 },
}

/// Action module for the `"podcast.settings"` namespace.
pub struct SettingsActionModule;

impl ActionModule for SettingsActionModule {
    const NAMESPACE: &'static str = "podcast.settings";

    type Action = SettingsAction;

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
    fn set_skip_intervals_round_trips() {
        let action = SettingsAction::SetSkipIntervals { forward_secs: 45.0, backward_secs: 10.0 };
        let json = serde_json::to_string(&action).expect("encode");
        assert!(json.contains(r#""op":"set_skip_intervals""#));
        assert!(json.contains(r#""forward_secs":45.0"#) || json.contains(r#""forward_secs":45"#));
        let decoded: SettingsAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, action);
    }

    #[test]
    fn set_auto_skip_ads_round_trips() {
        let action = SettingsAction::SetAutoSkipAds { enabled: true };
        let json = serde_json::to_string(&action).expect("encode");
        assert!(json.contains(r#""op":"set_auto_skip_ads""#));
        assert!(json.contains(r#""enabled":true"#));
        let decoded: SettingsAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, action);
    }

    #[test]
    fn execute_emits_dispatch_host_op() {
        let action = SettingsAction::SetAutoSkipAds { enabled: false };
        let commands = std::sync::Mutex::new(Vec::<ActorCommand>::new());
        SettingsActionModule::execute(action, "corr-1", &|cmd| {
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
        assert_eq!(v["op"], "set_auto_skip_ads");
        assert_eq!(v["enabled"], false);
    }
}
