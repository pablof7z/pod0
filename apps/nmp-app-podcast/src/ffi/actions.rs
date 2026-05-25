//! Podcast-specific action-registration helpers invoked from
//! [`super::register::nmp_app_podcast_register`].
//!
//! `nmp_app_podcast_register` calls `nmp_app_template::register_defaults` for
//! the canonical NMP action modules (NIP-02 / NIP-17 / NIP-57 / NIP-65) and
//! the production routing substrate. This file is the hook point for
//! **Podcast-specific** registrations that the template intentionally does not
//! ship — NIP-74 podcast feed actions, episode playback intents, chapter
//! navigation, etc.
//!
//! For M3.A the action *types* are defined here so the iOS shell has a
//! stable contract to encode. The kernel-side `ActionModule` registration
//! (which routes a dispatched action into a [`crate::player::PlayerActor`]
//! mutation and a follow-up [`crate::capability::AudioCommand`]) lands in
//! M3.B alongside the player projection's snapshot wiring.
//!
//! ## Wire shape
//!
//! Every player action carries a stable string id Swift can match on:
//!
//! ```text
//! podcast.player.play             — PlayAction      { episode_id: String }
//! podcast.player.pause            — PauseAction
//! podcast.player.seek             — SeekAction      { position_secs: f64 }
//! podcast.player.set_speed        — SetSpeedAction  { speed: f32 }
//! podcast.player.set_volume       — SetVolumeAction { volume: f32 }
//! podcast.player.set_sleep_timer  — SetSleepTimerAction { secs: Option<u64> }
//! podcast.player.stop             — StopAction
//! ```
//!
//! Each id is exposed as a `pub const` so the iOS shell, the lint gate,
//! and the future `ActionModule::action_id` impls reference one string.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Action id constants (kernel ↔ shell contract)
// ---------------------------------------------------------------------------

/// `podcast.player.play` — begin playback of `episode_id`.
pub const ACTION_PLAYER_PLAY: &str = "podcast.player.play";
/// `podcast.player.pause` — pause the active episode.
pub const ACTION_PLAYER_PAUSE: &str = "podcast.player.pause";
/// `podcast.player.seek` — seek the active episode.
pub const ACTION_PLAYER_SEEK: &str = "podcast.player.seek";
/// `podcast.player.set_speed` — change playback rate.
pub const ACTION_PLAYER_SET_SPEED: &str = "podcast.player.set_speed";
/// `podcast.player.set_volume` — change engine-level volume.
pub const ACTION_PLAYER_SET_VOLUME: &str = "podcast.player.set_volume";
/// `podcast.player.set_sleep_timer` — arm / cancel sleep timer.
pub const ACTION_PLAYER_SET_SLEEP_TIMER: &str = "podcast.player.set_sleep_timer";
/// `podcast.player.stop` — tear down the active episode.
pub const ACTION_PLAYER_STOP: &str = "podcast.player.stop";

// ---------------------------------------------------------------------------
// Action payloads
// ---------------------------------------------------------------------------

/// Payload for [`ACTION_PLAYER_PLAY`].
///
/// `episode_id` resolves to a queued episode in the podcast-domain
/// store; the kernel looks up its enclosure URL + last-known position
/// and dispatches `AudioCommand::Load { url, position_secs }` followed
/// by `AudioCommand::Play`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PlayAction {
    pub episode_id: String,
}

/// Payload for [`ACTION_PLAYER_PAUSE`]. Empty — pause always targets
/// the active episode.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct PauseAction;

/// Payload for [`ACTION_PLAYER_SEEK`].
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct SeekAction {
    pub position_secs: f64,
}

/// Payload for [`ACTION_PLAYER_SET_SPEED`]. Clamped to `0.5..=2.0` by
/// [`crate::player::PlayerActor::set_speed`].
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct SetSpeedAction {
    pub speed: f32,
}

/// Payload for [`ACTION_PLAYER_SET_VOLUME`]. Clamped to `0.0..=1.0` by
/// [`crate::player::PlayerActor::set_volume`].
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct SetVolumeAction {
    pub volume: f32,
}

/// Payload for [`ACTION_PLAYER_SET_SLEEP_TIMER`]. `Some(n)` arms a
/// timer of `n` seconds; `None` cancels any active timer.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SetSleepTimerAction {
    #[serde(default)]
    pub secs: Option<u64>,
}

/// Payload for [`ACTION_PLAYER_STOP`]. Empty — stop always targets
/// the active episode.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct StopAction;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_ids_match_documented_strings() {
        assert_eq!(ACTION_PLAYER_PLAY, "podcast.player.play");
        assert_eq!(ACTION_PLAYER_PAUSE, "podcast.player.pause");
        assert_eq!(ACTION_PLAYER_SEEK, "podcast.player.seek");
        assert_eq!(ACTION_PLAYER_SET_SPEED, "podcast.player.set_speed");
        assert_eq!(ACTION_PLAYER_SET_VOLUME, "podcast.player.set_volume");
        assert_eq!(
            ACTION_PLAYER_SET_SLEEP_TIMER,
            "podcast.player.set_sleep_timer"
        );
        assert_eq!(ACTION_PLAYER_STOP, "podcast.player.stop");
    }

    #[test]
    fn play_action_serde_roundtrips() {
        let a = PlayAction {
            episode_id: "ep-7".into(),
        };
        let json = serde_json::to_string(&a).expect("encode");
        assert_eq!(json, r#"{"episode_id":"ep-7"}"#);
        let decoded: PlayAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, a);
    }

    #[test]
    fn sleep_timer_action_handles_some_and_none() {
        let arm = SetSleepTimerAction { secs: Some(1800) };
        let json = serde_json::to_string(&arm).expect("encode");
        assert_eq!(json, r#"{"secs":1800}"#);

        let cancel = SetSleepTimerAction::default();
        let json = serde_json::to_string(&cancel).expect("encode");
        assert_eq!(json, r#"{"secs":null}"#);

        // Absent `secs` (the iOS encoder may omit `null`) decodes as None.
        let decoded: SetSleepTimerAction = serde_json::from_str("{}").expect("decode");
        assert!(decoded.secs.is_none());
    }

    #[test]
    fn seek_action_serde_roundtrips() {
        let a = SeekAction {
            position_secs: 42.5,
        };
        let json = serde_json::to_string(&a).expect("encode");
        let decoded: SeekAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, a);
    }
}
