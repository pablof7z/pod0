//! Podcast-specific action-registration helpers invoked from
//! [`super::register::nmp_app_podcast_register`].
//!
//! `nmp_app_podcast_register` calls `nmp_defaults::register_defaults` for
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
//! podcast.agent.send                   — SendAgentMessageAction  { conversation_id?, message }
//! podcast.agent.approve                — ApproveAction           { approval_id }
//! podcast.agent.deny                   — DenyAction              { approval_id, reason? }
//! podcast.agent.clear                  — ClearConversationAction { conversation_id }
//! podcast.siri.play_latest             — SiriPlayLatestAction    { podcast_id? }
//! podcast.siri.resume                  — SiriResumeAction
//! ```
//!
//! Each id is exposed as a `pub const` so the iOS shell, the lint gate,
//! and the future `ActionModule::action_id` impls reference one string.
//!
//! ## Module layout
//!
//! Player actions live in this `mod.rs`. Voice actions live in
//! [`voice`]. Agent actions are re-exported from `podcast-agent-core`.
//! Each domain owns its wire format; this module is the single import path the
//! iOS shell links against.

pub mod agent_module;
pub(crate) mod categorization_keywords;
pub mod categorization_module;
pub mod chapters_module;
pub mod clip_module;
pub mod identity_module;
pub mod inbox_module;
pub mod knowledge_module;
pub mod memory_module;
pub mod picks_module;
pub mod player_module;
pub mod podcast_module;
pub mod publish_module;
pub mod queue_module;
pub mod settings_module;
pub mod siri_module;
pub mod social_module;
pub mod tasks_module;
pub mod voice;
pub mod voice_module;

pub use agent_module::{AgentActionModule, AgentChatAction};
pub use categorization_module::{
    categorize_text, CategorizationAction, CategorizationModule, ACTION_CATEGORIZE_EPISODE,
    ACTION_CATEGORIZE_RUN, CATEGORY_KEYWORDS, MAX_CATEGORIES_PER_EPISODE,
};
pub use chapters_module::{ChaptersAction, ChaptersActionModule};
pub use clip_module::{
    ClipAction, ClipActionModule, ACTION_CLIP_AUTO_SNIP, ACTION_CLIP_CREATE, ACTION_CLIP_DELETE,
};
pub use identity_module::{IdentityAction, IdentityActionModule};
pub use inbox_module::{InboxAction, InboxActionModule};
pub use knowledge_module::{
    KnowledgeAction, KnowledgeActionModule, ACTION_KNOWLEDGE_CLEAR_RESULTS,
    ACTION_KNOWLEDGE_INDEX_EPISODE, ACTION_KNOWLEDGE_SEARCH,
};
pub use memory_module::{MemoryAction, MemoryActionModule};
pub use picks_module::{AgentPicksModule, PicksAction, PICKS_LIMIT, PICKS_PER_SHOW_CAP};
pub use player_module::{PlayerAction, PlayerActionModule};
pub use podcast_module::{PodcastAction, PodcastActionModule};
pub use publish_module::{
    NipF4PublishModule, PublishAction, ACTION_PUBLISH_CREATE_OWNED,
    ACTION_PUBLISH_PUBLISH_AUTHOR_CLAIM, ACTION_PUBLISH_PUBLISH_EPISODE,
    ACTION_PUBLISH_PUBLISH_SHOW, ACTION_PUBLISH_REMOVE_OWNED,
};
pub use queue_module::{QueueAction, QueueActionModule};
pub use settings_module::{SettingsAction, SettingsActionModule};
pub use siri_module::{SiriAction, SiriActionModule};
pub use social_module::{
    SocialAction, SocialActionModule, ACTION_SOCIAL_PUBLISH_HIGHLIGHT, ACTION_SOCIAL_PUBLISH_NOTE,
    ACTION_SOCIAL_PUBLISH_PROFILE,
};
pub use tasks_module::{AgentTaskIntent, AgentTasksAction, AgentTasksModule};
pub use voice_module::{VoiceAction, VoiceActionModule};

use serde::{Deserialize, Serialize};

/// Wrap `action` in a namespaced envelope and forward it as a
/// `DispatchHostOp`. The host-op handler peels `ns` to route to the
/// correct handler arm, eliminating the try-parse cascade.
///
/// Envelope shape: `{"ns":"<namespace>","action":<bare action JSON>}`.
pub(crate) fn dispatch_host_op(
    ns: &str,
    action: &impl Serialize,
    correlation_id: &str,
    send: &dyn Fn(nmp_core::ActorCommand),
) -> Result<(), String> {
    let envelope = serde_json::json!({ "ns": ns, "action": action });
    send(nmp_core::ActorCommand::Protocol(Box::new(
        nmp_core::substrate::host_op_command(
            envelope.to_string(),
            correlation_id.to_owned(),
        ),
    )));
    Ok(())
}

pub use voice::{
    SetVoiceAction, SpeakAction, StopVoiceAction, ACTION_VOICE_SET_VOICE, ACTION_VOICE_SPEAK,
    ACTION_VOICE_STOP,
};

/// `podcast.voice.activate` — enter voice mode (begin STT).
pub const ACTION_VOICE_ACTIVATE: &str = "podcast.voice.activate";
/// `podcast.voice.deactivate` — exit voice mode (stop STT).
pub const ACTION_VOICE_DEACTIVATE: &str = "podcast.voice.deactivate";

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
/// `podcast.player.skip_forward` — relative seek forward by `secs` seconds.
pub const ACTION_PLAYER_SKIP_FORWARD: &str = "podcast.player.skip_forward";
/// `podcast.player.skip_backward` — relative seek back by `secs` seconds (clamped to 0).
pub const ACTION_PLAYER_SKIP_BACKWARD: &str = "podcast.player.skip_backward";

// ---------------------------------------------------------------------------
// Inbox action id constants (kernel ↔ shell contract — feature #31)
// ---------------------------------------------------------------------------

/// `podcast.inbox.triage` — recompute the inbox projection.
pub const ACTION_INBOX_TRIAGE: &str = "podcast.inbox.triage";
/// `podcast.inbox.dismiss` — remove an episode from the inbox.
pub const ACTION_INBOX_DISMISS: &str = "podcast.inbox.dismiss";
/// `podcast.inbox.mark_listened` — mark an episode as played.
pub const ACTION_INBOX_MARK_LISTENED: &str = "podcast.inbox.mark_listened";

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

// Wire-contract tests live in `actions_tests.rs` to keep this file under
// the 500-line hard limit (AGENTS.md).
#[cfg(test)]
#[path = "actions_tests.rs"]
mod tests;
