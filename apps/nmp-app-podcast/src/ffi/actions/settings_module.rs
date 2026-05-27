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
    /// Toggle auto-play-next: when `true`, the kernel auto-advances to
    /// the next queued episode on natural `ItemEnd`.
    SetAutoPlayNext { enabled: bool },
    /// Toggle auto-mark-played: when `true`, the kernel marks the episode
    /// listened on natural `ItemEnd`.
    SetAutoMarkPlayedAtEnd { enabled: bool },
    /// Update both headphone gesture action strings. The raw values must
    /// match a `HeadphoneGestureAction` case on the iOS side.
    SetHeadphoneGestureActions { double_tap: String, triple_tap: String },
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
#[path = "settings_module_tests.rs"]
mod tests;
