//! [`PlayerState`] — the projected player snapshot consumed by the UI
//! shell via `PodcastUpdate.now_playing`.
//!
//! Kept in a separate file from [`super::PlayerActor`] so the
//! projection's wire shape stays close to its doc-comment narrative —
//! and so editing the state-machine logic doesn't churn the file with
//! the public type the iOS decoder reads from.

use serde::{Deserialize, Serialize};

/// Public player projection — surfaced via `PodcastUpdate.now_playing`.
///
/// `Default::default()` corresponds to "nothing loaded, not playing":
/// every numeric field is zero; every `Option` is `None`. Per the
/// snapshot doctrine from the active NMP feature-parity plan, the
/// kernel should serialize `None` for the whole struct when nothing is
/// queued, not a struct full of zeros — but when an episode *is*
/// loaded, every field is meaningful.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct PlayerState {
    /// Currently-loaded episode id, when known. `None` before any
    /// `Load` action has been dispatched.
    pub episode_id: Option<String>,
    /// Show / podcast id the active episode belongs to.
    pub podcast_id: Option<String>,
    /// Resolved enclosure URL the iOS capability is playing.
    pub url: Option<String>,
    /// Current playhead in seconds from the start of the track.
    pub position_secs: f64,
    /// Track duration in seconds; `0.0` until the capability reports it.
    pub duration_secs: f64,
    /// `true` iff the most recent `AudioReport` was `Playing`.
    pub is_playing: bool,
    /// Playback rate in `0.5..=3.0`. Defaults to `1.0`.
    pub speed: f32,
    /// Engine-level volume in `0.0..=1.0`. Defaults to `1.0`.
    pub volume: f32,
    /// Wall-clock seconds remaining on the sleep timer, when armed.
    /// Recomputed on every report from the stored deadline.
    pub sleep_timer_remaining_secs: Option<u64>,
    /// Buffering progress in `0.0..=1.0` from the most recent
    /// `BufferingProgress` report; `None` once playback resumes.
    pub buffering_fraction: Option<f32>,
    /// Most recent capability error (`AudioReport::Failed.error`).
    /// Cleared on the next successful `Load`. Surfaces in the
    /// `now_playing` projection so the UI can render a banner.
    pub last_error: Option<String>,
}

impl PlayerState {
    /// "Fresh, idle, defaults" — `speed = 1.0`, `volume = 1.0`, every
    /// other field zero / `None`. Distinct from `Default::default()`
    /// only in that the rate and volume start at "neutral" rather than
    /// zero (a zero-rate `AVPlayer` is paused, not idle).
    #[must_use]
    pub fn idle() -> Self {
        Self {
            speed: 1.0,
            volume: 1.0,
            ..Self::default()
        }
    }
}
