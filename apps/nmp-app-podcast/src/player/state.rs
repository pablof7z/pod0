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
    /// Set to `true` when the audio capability reports `ItemEnd`
    /// (AVPlayerItemDidPlayToEndTime). Cleared when the next `Load` stages.
    /// The UI uses this to distinguish a natural finish from a user-initiated
    /// stop, and M1.3 business logic gates auto-advance on it.
    #[serde(default)]
    pub did_reach_natural_end: bool,
    /// Absolute end boundary for a bounded agent segment. When set, the
    /// player should treat reaching this position as an `ItemEnd` (auto-
    /// advance, mark played, etc.). Cleared when the episode loads or a
    /// new segment boundary is assigned. `None` for unbounded playback.
    #[serde(default)]
    pub segment_end_secs: Option<f64>,
    /// Title of the chapter active at the current playhead position,
    /// sourced from the store's chapter list. `None` when the episode has
    /// no chapters or the position is before the first chapter start.
    #[serde(default)]
    pub current_chapter_title: Option<String>,
    /// Artwork URL override for the active chapter (per-chapter `<itunes:image>`).
    /// Overrides the episode and show artwork while the chapter is active.
    /// `None` when the chapter has no per-chapter image, or when there are no chapters.
    #[serde(default)]
    pub current_chapter_artwork_url: Option<String>,
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
