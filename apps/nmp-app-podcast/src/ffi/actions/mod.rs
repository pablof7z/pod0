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
//! Every action carries a stable string id Swift can match on:
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
//! podcast.voice.speak                  — SpeakAction             { text, voice_id? }
//! podcast.voice.stop                   — StopVoiceAction
//! podcast.voice.set_voice              — SetVoiceAction          { voice_id }
//! podcast.briefing.request             — RequestBriefingAction
//! podcast.briefing.schedule            — ScheduleBriefingAction  { schedule }
//! podcast.briefing.cancel              — CancelBriefingAction
//! podcast.agent.send                   — SendAgentMessageAction  { conversation_id?, message }
//! podcast.agent.approve                — ApproveAction           { approval_id }
//! podcast.agent.deny                   — DenyAction              { approval_id, reason? }
//! podcast.agent.clear                  — ClearConversationAction { conversation_id }
//! podcast.siri.play_latest             — SiriPlayLatestAction    { podcast_id? }
//! podcast.siri.resume                  — SiriResumeAction
//! podcast.wiki.generate                — WikiAction::Generate     { podcast_id, topic }
//! podcast.wiki.delete                  — WikiAction::Delete       { article_id }
//! podcast.wiki.search                  — WikiAction::Search       { query }
//! ```
//!
//! Each id is exposed as a `pub const` so the iOS shell, the lint gate,
//! and the future `ActionModule::action_id` impls reference one string.
//!
//! ## Module layout
//!
//! Player actions live in this `mod.rs`. Voice actions live in
//! [`voice`]. Briefing actions are re-exported from `podcast-briefings`;
//! agent actions are re-exported from `podcast-agent-core`. Each domain
//! owns its wire format; this module is the single import path the
//! iOS shell links against.

pub mod chapters_module;
pub mod picks_module;
pub mod player_module;
pub mod podcast_module;
pub mod queue_module;
pub mod tasks_module;
pub mod voice;
pub mod wiki_module;

pub use chapters_module::{ChaptersAction, ChaptersActionModule};
pub use picks_module::{AgentPicksModule, PicksAction, PICKS_LIMIT, PICKS_PER_SHOW_CAP};
pub use player_module::{PlayerAction, PlayerActionModule};
pub use podcast_module::{PodcastAction, PodcastActionModule};
pub use queue_module::{QueueAction, QueueActionModule};
pub use wiki_module::{WikiAction, WikiActionModule};
pub use tasks_module::{AgentTasksAction, AgentTasksModule};

use serde::{Deserialize, Serialize};

pub use voice::{
    SetVoiceAction, SpeakAction, StopVoiceAction, ACTION_VOICE_SET_VOICE, ACTION_VOICE_SPEAK,
    ACTION_VOICE_STOP,
};

// ---------------------------------------------------------------------------
// Player action id constants (kernel ↔ shell contract)
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

// Siri / AppIntents action ids (M11 platform-integration contract)
// ---------------------------------------------------------------------------
//
// These ids are dispatched by iOS `AppIntents` performers (e.g. the
// `StartVoiceModeIntent` ⇒ `play_latest`) and by Siri shortcut donations.
// Per D7 the iOS side only **dispatches** the intent; the kernel decides
// what "play latest" actually means (which podcast, which episode, what to
// do if nothing is queued). The intent performers carry no policy.

/// `podcast.siri.play_latest` — play the latest episode for the
/// optionally-supplied podcast (or, when omitted, across the user's
/// whole library). Dispatched from a Siri shortcut donation or an
/// AppIntent invocation.
pub const ACTION_SIRI_PLAY_LATEST: &str = "podcast.siri.play_latest";

/// `podcast.siri.resume` — resume whatever was last playing. The
/// kernel looks up the previously-active episode + position and
/// dispatches the equivalent `AudioCommand::Load` + `Play` pair.
pub const ACTION_SIRI_RESUME: &str = "podcast.siri.resume";

// ---------------------------------------------------------------------------
// Player action payloads
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

/// Payload for [`ACTION_PLAYER_SET_SPEED`]. Clamped to `0.5..=3.0` by
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

/// Payload for [`ACTION_SIRI_PLAY_LATEST`].
///
/// `podcast_id` is optional: when set, the kernel plays the latest
/// episode for that podcast; when omitted, the kernel picks across
/// the whole library (typically the most recently published episode
/// from any subscribed show).
///
/// Carried as `Option<String>` (not typed `PodcastId`) so Siri's
/// shortcut donor can pass either a hand-picked id or no id at all
/// without round-tripping through a domain-typed encoder.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct SiriPlayLatestAction {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub podcast_id: Option<String>,
}

/// Payload for [`ACTION_SIRI_RESUME`]. Empty — resume always targets
/// the most-recently-active episode; if there isn't one, the kernel
/// emits a `toast` on the snapshot and does nothing else.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct SiriResumeAction;

// ---------------------------------------------------------------------------
// Briefing actions (re-exported from `podcast-briefings` for M9.A)
// ---------------------------------------------------------------------------
//
// The briefing action ids + payloads live in `podcast-briefings` so the
// crate that owns the briefing/scheduler domain also owns its wire
// format. Re-exported through this module so the iOS shell links against
// `nmp_app_podcast::ffi::actions::ACTION_BRIEFING_*` exactly like the
// player / agent / voice actions — one import path for every action
// contract.
pub use podcast_briefings::{
    CancelBriefingAction, RequestBriefingAction, ScheduleBriefingAction, ACTION_BRIEFING_CANCEL,
    ACTION_BRIEFING_REQUEST, ACTION_BRIEFING_SCHEDULE,
};

// ---------------------------------------------------------------------------
// Agent actions (re-exported from `podcast-agent-core` for M7.A)
// ---------------------------------------------------------------------------
//
// The agent-chat action ids + payloads live in `podcast-agent-core` so the
// crate that owns the conversation/approval domain also owns its wire
// format. Re-exported through this module so the iOS shell links against
// `nmp_app_podcast::ffi::actions::ACTION_AGENT_*` exactly like the player
// actions above — one import path for every action contract.
pub use podcast_agent_core::{
    ApproveAction as AgentApproveAction, ClearConversationAction as AgentClearConversationAction,
    DenyAction as AgentDenyAction, SendAgentMessageAction, ACTION_AGENT_APPROVE,
    ACTION_AGENT_CLEAR, ACTION_AGENT_DENY, ACTION_AGENT_SEND,
};

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

    // ── Siri / AppIntents action contract (M11) ─────────────────────

    #[test]
    fn siri_action_ids_match_documented_strings() {
        assert_eq!(ACTION_SIRI_PLAY_LATEST, "podcast.siri.play_latest");
        assert_eq!(ACTION_SIRI_RESUME, "podcast.siri.resume");
    }

    #[test]
    fn siri_play_latest_action_round_trips_with_podcast_id() {
        let a = SiriPlayLatestAction {
            podcast_id: Some("pod-1".into()),
        };
        let json = serde_json::to_string(&a).expect("encode");
        assert_eq!(json, r#"{"podcast_id":"pod-1"}"#);
        let decoded: SiriPlayLatestAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, a);
    }

    #[test]
    fn siri_play_latest_action_omits_none_podcast_id() {
        let a = SiriPlayLatestAction::default();
        let json = serde_json::to_string(&a).expect("encode");
        assert_eq!(json, "{}");
        // Bare `{}` decodes as the unset variant.
        let decoded: SiriPlayLatestAction = serde_json::from_str("{}").expect("decode");
        assert!(decoded.podcast_id.is_none());
    }

    #[test]
    fn siri_resume_action_is_unit_struct() {
        let a = SiriResumeAction;
        let json = serde_json::to_string(&a).expect("encode");
        assert_eq!(json, "null");
    }

    // ── Agent action re-export contract (M7.A) ──────────────────────

    #[test]
    fn agent_action_ids_match_documented_strings() {
        assert_eq!(ACTION_AGENT_SEND, "podcast.agent.send");
        assert_eq!(ACTION_AGENT_APPROVE, "podcast.agent.approve");
        assert_eq!(ACTION_AGENT_DENY, "podcast.agent.deny");
        assert_eq!(ACTION_AGENT_CLEAR, "podcast.agent.clear");
    }

    #[test]
    fn agent_send_action_round_trips_through_reexport() {
        let a = SendAgentMessageAction {
            conversation_id: Some("c1".into()),
            message: "hi".into(),
        };
        let json = serde_json::to_string(&a).expect("encode");
        let decoded: SendAgentMessageAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, a);
    }

    // ── Briefing actions (M9.A — re-exports) ─────────────────────────

    #[test]
    fn briefing_action_ids_match_documented_strings() {
        assert_eq!(ACTION_BRIEFING_REQUEST, "podcast.briefing.request");
        assert_eq!(ACTION_BRIEFING_SCHEDULE, "podcast.briefing.schedule");
        assert_eq!(ACTION_BRIEFING_CANCEL, "podcast.briefing.cancel");
    }

    #[test]
    fn briefing_request_action_round_trips_through_reexport() {
        let a = RequestBriefingAction;
        let json = serde_json::to_string(&a).expect("encode");
        let decoded: RequestBriefingAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, a);
    }

    #[test]
    fn briefing_cancel_action_round_trips_through_reexport() {
        let a = CancelBriefingAction;
        let json = serde_json::to_string(&a).expect("encode");
        let decoded: CancelBriefingAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, a);
    }
}
