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
//! podcast.player.play                  — PlayAction              { episode_id: String }
//! podcast.player.pause                 — PauseAction
//! podcast.player.seek                  — SeekAction              { position_secs: f64 }
//! podcast.player.set_speed             — SetSpeedAction          { speed: f32 }
//! podcast.player.set_volume            — SetVolumeAction         { volume: f32 }
//! podcast.player.set_sleep_timer       — SetSleepTimerAction     { secs: Option<u64> }
//! podcast.player.stop                  — StopAction
//! podcast.player.download              — DownloadEpisodeAction   { episode_id, url }
//! podcast.player.cancel_download       — CancelDownloadAction    { episode_id }
//! podcast.player.pause_download        — PauseDownloadAction     { episode_id }
//! podcast.player.resume_download       — ResumeDownloadAction    { episode_id }
//! podcast.player.cancel_all_downloads  — CancelAllDownloadsAction
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
/// `podcast.player.download` — enqueue an episode for background download.
pub const ACTION_PLAYER_DOWNLOAD: &str = "podcast.player.download";
/// `podcast.player.cancel_download` — cancel an active or queued download.
pub const ACTION_PLAYER_CANCEL_DOWNLOAD: &str = "podcast.player.cancel_download";
/// `podcast.player.pause_download` — pause an active download.
pub const ACTION_PLAYER_PAUSE_DOWNLOAD: &str = "podcast.player.pause_download";
/// `podcast.player.resume_download` — resume a paused download.
pub const ACTION_PLAYER_RESUME_DOWNLOAD: &str = "podcast.player.resume_download";
/// `podcast.player.cancel_all_downloads` — cancel every in-flight + queued download.
pub const ACTION_PLAYER_CANCEL_ALL_DOWNLOADS: &str = "podcast.player.cancel_all_downloads";

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

/// Payload for [`ACTION_PLAYER_DOWNLOAD`].
///
/// `episode_id` resolves to an episode the user wants downloaded; `url`
/// is the enclosure to fetch. The kernel routes this to
/// [`crate::download::DownloadQueue::enqueue`] and (when slot-available)
/// dispatches `DownloadCommand::StartDownload` to the iOS executor.
///
/// `url` is carried in the action rather than re-derived in the kernel
/// so the action remains pure data (no implicit episode-store lookup
/// from the action module). M4.B's auto-download policy fills both
/// fields from the episode record it iterates over.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct DownloadEpisodeAction {
    pub episode_id: String,
    pub url: String,
}

/// Payload for [`ACTION_PLAYER_CANCEL_DOWNLOAD`].
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CancelDownloadAction {
    pub episode_id: String,
}

/// Payload for [`ACTION_PLAYER_PAUSE_DOWNLOAD`].
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct PauseDownloadAction {
    pub episode_id: String,
}

/// Payload for [`ACTION_PLAYER_RESUME_DOWNLOAD`].
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ResumeDownloadAction {
    pub episode_id: String,
}

/// Payload for [`ACTION_PLAYER_CANCEL_ALL_DOWNLOADS`]. Empty — there is
/// nothing to target beyond "all in-flight + queued".
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct CancelAllDownloadsAction;

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
        assert_eq!(ACTION_PLAYER_DOWNLOAD, "podcast.player.download");
        assert_eq!(
            ACTION_PLAYER_CANCEL_DOWNLOAD,
            "podcast.player.cancel_download"
        );
        assert_eq!(
            ACTION_PLAYER_PAUSE_DOWNLOAD,
            "podcast.player.pause_download"
        );
        assert_eq!(
            ACTION_PLAYER_RESUME_DOWNLOAD,
            "podcast.player.resume_download"
        );
        assert_eq!(
            ACTION_PLAYER_CANCEL_ALL_DOWNLOADS,
            "podcast.player.cancel_all_downloads"
        );
    }

    #[test]
    fn download_episode_action_serde_roundtrips() {
        let a = DownloadEpisodeAction {
            episode_id: "ep-7".into(),
            url: "https://ex.com/7.mp3".into(),
        };
        let json = serde_json::to_string(&a).expect("encode");
        let decoded: DownloadEpisodeAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, a);
    }

    #[test]
    fn cancel_download_action_serde_roundtrips() {
        let a = CancelDownloadAction {
            episode_id: "ep-7".into(),
        };
        let json = serde_json::to_string(&a).expect("encode");
        assert_eq!(json, r#"{"episode_id":"ep-7"}"#);
        let decoded: CancelDownloadAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, a);
    }

    #[test]
    fn pause_resume_download_actions_round_trip() {
        let pause = PauseDownloadAction {
            episode_id: "ep-7".into(),
        };
        let resume = ResumeDownloadAction {
            episode_id: "ep-7".into(),
        };
        let pj = serde_json::to_string(&pause).expect("encode");
        let rj = serde_json::to_string(&resume).expect("encode");
        let pd: PauseDownloadAction = serde_json::from_str(&pj).expect("decode");
        let rd: ResumeDownloadAction = serde_json::from_str(&rj).expect("decode");
        assert_eq!(pd, pause);
        assert_eq!(rd, resume);
    }

    #[test]
    fn cancel_all_downloads_action_is_unit_struct() {
        let a = CancelAllDownloadsAction;
        let json = serde_json::to_string(&a).expect("encode");
        assert_eq!(json, "null");
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
// This is a placeholder for M0. Podcast-domain action modules will be added
// here in subsequent milestones as the corresponding NIP crates are
// implemented.
//
// ## M2.D — legacy migration wiring (TODO)
//
// The `pcst.legacy_io.capability` (iOS-side `LegacyIOCapability.swift`,
// domain logic in `podcast_core::migration`) needs a kernel-side caller
// that, on first launch:
//
// 1. Issues a `migration_done_read` request. If `done == true`, stop —
//    migration is a no-op (idempotence, per M2.D quality gate).
// 2. Issues `read_state_json`. Base64-decodes the payload and hands it to
//    `podcast_core::migration::from_state_json`.
// 3. Issues `read_episode_db` (stub — `from_episode_db` currently returns
//    `EpisodeDbUnsupported`; the kernel logs and proceeds).
// 4. Folds the resulting `MigrationResult` into the snapshot's podcasts +
//    subscriptions stores.
// 5. Issues `migration_done_set`. Per D6, any error along the way leaves
//    the sentinel UNSET and surfaces a `toast: Option<String>` on the
//    snapshot — the next launch retries.
//
// This wiring lands when the kernel-side `nmp-store` integration plus the
// podcast-projection observer arrive in M2.E. Until then, the capability
// is dormant on the iOS side: started during app boot, never invoked by
// Rust, no behaviour change observable to the user.
