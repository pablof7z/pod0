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
use nmp_core::actor::ActorCommand;

use crate::player::AdSegment;

/// Wire enum for all `"podcast.player"` namespace actions.
///
/// `#[serde(tag = "op", rename_all = "snake_case")]` makes the JSON
/// discriminator the lowercase snake-case variant name:
/// `play` → `{"op":"play","episode_id":"..."}`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PlayerAction {
    Play {
        episode_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_secs: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        end_secs: Option<f64>,
    },
    /// Stage an episode for playback without starting audio. Rust looks up
    /// the URL and position, calls `actor.stage_load`, and dispatches
    /// `AudioCommand::Load` — but NOT `AudioCommand::Play`. iOS follows
    /// with a `Resume` action (or a `Play { episode_id }` to restart).
    Load {
        episode_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_secs: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        end_secs: Option<f64>,
    },
    /// Resume playback of the currently-staged episode. Dispatches
    /// `AudioCommand::Play` only — no reload, no position reset.
    Resume,
    Pause,
    Seek {
        position_secs: f64,
    },
    SetSpeed {
        speed: f32,
    },
    SetVolume {
        volume: f32,
    },
    SetSleepTimer {
        #[serde(default)]
        secs: Option<u64>,
        #[serde(default)]
        end_of_episode: bool,
    },
    Stop,
    /// Append `episode_id` to the end of the playback queue if not
    /// already present (dedup by id). Kernel-owned ordered list of
    /// episode ids surfaced via `PodcastUpdate.queue`.
    Enqueue {
        episode_id: String,
    },
    /// Insert `episode_id` at the front of the playback queue if not already
    /// present. Kernel-owned ordered list of episode ids surfaced via
    /// `PodcastUpdate.queue`.
    EnqueueNext {
        episode_id: String,
    },
    /// Append a bounded episode segment to the end of the playback queue.
    EnqueueSegment {
        episode_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_secs: Option<f64>,
        end_secs: f64,
    },
    /// Insert a bounded episode segment at the front of the playback queue.
    EnqueueSegmentNext {
        episode_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_secs: Option<f64>,
        end_secs: f64,
    },
    /// Remove the first occurrence of `episode_id` from the queue.
    Dequeue {
        episode_id: String,
    },
    /// Remove one queue slot by Rust-owned queue slot id.
    DequeueSlot {
        queue_slot_id: String,
    },
    /// Reorder existing queue slots by Rust-owned queue slot ids.
    ReorderQueue {
        queue_slot_ids: Vec<String>,
    },
    /// Empty the entire playback queue.
    ClearQueue,
    /// Pop the front of the queue and `Play` it. No-op when the queue
    /// is empty.
    PlayNext,
    /// Set the ad-break list for `episode_id`. Stored in the side-map
    /// on `PodcastStore` and (when the episode is the one currently
    /// loaded) pushed into the player actor so auto-skip can fire on
    /// the next `Playing` tick.
    ///
    /// Carries the full vec rather than incrementally adding so the
    /// caller (an ingest pipeline upstream) is the single owner of
    /// the segment list — re-running detection always emits the
    /// canonical replacement, never a diff.
    SetAdSegments {
        episode_id: String,
        segments: Vec<AdSegment>,
    },
    /// Advance the playhead by `secs` seconds from the current position.
    /// The kernel reads the live `PlayerActor` position so the iOS/Android
    /// shell never needs to know the current time (D0 — policy in Rust).
    SkipForward {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        secs: Option<f64>,
    },
    /// Step the playhead back by `secs` seconds (clamped to 0).
    SkipBackward {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        secs: Option<f64>,
    },
    /// Enqueue an episode audio file for offline download.
    Download {
        episode_id: String,
        url: String,
    },
    /// Cancel an active, paused, or queued download.
    CancelDownload {
        episode_id: String,
    },
    /// Pause an active download while retaining resume data.
    PauseDownload {
        episode_id: String,
    },
    /// Resume a paused download.
    ResumeDownload {
        episode_id: String,
    },
    /// Cancel every active, paused, and queued download.
    CancelAllDownloads,
    /// Reset the playback position of an episode to zero. Clears the
    /// "Continue Listening" resume point without marking the episode played.
    ResetProgress {
        episode_id: String,
    },
    /// Pop the front of the queue and play it. Equivalent to `PlayNext`
    /// but named for auto-advance semantics — fired by Rust's `ItemEnd`
    /// handler; never synthesized by the iOS shell.
    Advance,
    /// Write a playback position to the store without going through the
    /// audio-report path. Used for deep-link warm-resume and mini-player
    /// restore where no `Playing` report is in flight.
    PersistPosition {
        episode_id: String,
        position_secs: f64,
    },
}

/// Action module for the `"podcast.player"` namespace.
///
/// `execute` serializes the typed `PlayerAction` back to JSON and hands it
/// to the actor as `ActorCommand::DispatchHostOp`. The installed
/// `PodcastHostOpHandler` deserializes it, dispatches the matching
/// `AudioCommand` to the audio capability, and returns a `{"ok":true}` envelope.
pub struct PlayerActionModule;

impl ActionModule for PlayerActionModule {
    const NAMESPACE: nmp_core::substrate::DeclaredActionNamespace =
        nmp_core::substrate::DeclaredActionNamespace::app_owned("podcast.player");

    type Action = PlayerAction;

    fn is_async_completing() -> bool {
        false
    }

    fn execute(
        &self,
        _ctx: &nmp_core::substrate::ActionContext,
        action: Self::Action,
        correlation_id: &str,
        send: &dyn Fn(ActorCommand),
    ) -> Result<(), String> {
        crate::ffi::actions::dispatch_host_op(Self::NAMESPACE.as_str(), &action, correlation_id, send)
    }

    fn decode_payload(
        bytes: &[u8],
    ) -> Option<Result<Self::Action, nmp_core::substrate::ActionPayloadDecodeError>> {
        crate::action_payload::decode_podcast_payload(bytes)
    }
}

#[cfg(test)]
#[path = "player_module_tests.rs"]
mod tests;
