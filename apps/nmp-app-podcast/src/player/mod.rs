//! Pure player state machine.
//!
//! [`PlayerActor`] owns the canonical [`PlayerState`] and projects
//! [`AudioReport`] events from the iOS audio capability into state
//! mutations, optionally emitting a follow-up [`AudioCommand`]
//! (e.g. `Stop` when a sleep timer expires).
//!
//! ## Pure
//!
//! `PlayerActor` is **synchronous** and **side-effect-free**: no async, no
//! channels, no I/O, no clock — every input is an explicit argument
//! (the report and a "now" instant for sleep-timer accounting). This is
//! deliberate: the FFI layer (`ffi/mod.rs`) handles async dispatch and
//! capability I/O; this module is straight state-machine code that's
//! cheap to unit-test.
//!
//! ## Doctrine
//!
//! * **D7 — Rust decides.** The iOS executor reports "AVPlayer is playing
//!   at 12.3s"; this module decides whether that means the player's view
//!   model should switch to `is_playing = true`, whether the sleep
//!   timer's deadline has elapsed, whether a `Stop` command should
//!   chase the report back. iOS doesn't ask "should I stop now?" — it
//!   reports a `Playing` event, and Rust independently checks the
//!   sleep-timer deadline.
//! * **D9 — kernel owns time.** Sleep-timer expiry is decided here from
//!   a caller-supplied `now: SystemTime` so tests can pin the clock.
//!   The iOS executor *also* schedules a local timer so it can fade the
//!   lock-screen volume, but the authoritative "are we past the
//!   deadline?" answer comes from this module.

use std::collections::HashSet;
use std::time::{Duration, SystemTime};

mod ad_segments;
mod audio_report;
mod state;
pub use ad_segments::AdSegment;
pub use state::PlayerState;

/// Pure projector over [`PlayerState`].
///
/// All methods take `&mut self` and return any follow-up
/// [`AudioCommand`] the FFI layer should dispatch back through the
/// capability. There is no internal clock — callers supply `now` so
/// tests can pin time.
#[derive(Clone, Debug, Default)]
pub struct PlayerActor {
    state: PlayerState,
    /// Wall-clock instant at which the sleep timer should fire, when
    /// armed. `None` outside of timer mode.
    sleep_deadline: Option<SystemTime>,
    /// Stop at the next natural episode end instead of auto-advancing.
    sleep_timer_end_of_episode: bool,
    /// Ad-break intervals for the currently-loaded episode. Set by
    /// the FFI layer at `play` time (and via `set_ad_segments`); empty
    /// when the upstream ingest hasn't annotated this episode.
    ad_segments: Vec<AdSegment>,
    /// User toggle for `auto_skip_ads`. When `true` and the playhead
    /// falls inside an [`AdSegment`] not yet in `skipped_ad_ids`, the
    /// `Playing` handler emits an `AudioCommand::Seek` past it.
    auto_skip_ads: bool,
    /// Ad ids the actor has auto-skipped during the current playback
    /// session. Cleared on `AudioReport::Stopped` (the actor's
    /// authoritative end-of-session signal). A user who scrubs back
    /// into a previously-skipped ad won't be auto-yanked forward — we
    /// treat scrub-back as "I want to hear this".
    skipped_ad_ids: HashSet<uuid::Uuid>,
    /// Mirror of `PodcastStore::auto_play_next`. When `true` and the
    /// queue is non-empty, the FFI layer auto-advances on `ItemEnd`.
    /// Pushed from the store at `play` time (same pattern as `auto_skip_ads`).
    pub(crate) auto_play_next: bool,
    /// Mirror of `PodcastStore::auto_mark_played_at_end`. When `true`,
    /// the writeback layer marks the episode listened on `ItemEnd`.
    /// Pushed from the store at `play` time.
    pub(crate) auto_mark_played_at_end: bool,
    /// Set for one report turn when a bounded segment reaches its Rust-owned
    /// end boundary. The FFI layer consumes this to decide whether to advance
    /// the canonical queue or send the native executor a plain Stop.
    segment_end_reached: bool,
}

impl PlayerActor {
    /// Construct an actor in the "idle, neutral defaults" state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: PlayerState::idle(),
            sleep_deadline: None,
            sleep_timer_end_of_episode: false,
            ad_segments: Vec::new(),
            auto_skip_ads: false,
            skipped_ad_ids: HashSet::new(),
            auto_play_next: true,
            auto_mark_played_at_end: true,
            segment_end_reached: false,
        }
    }

    /// Replace the active episode's ad-break intervals. Resets the
    /// per-session "already skipped" set so the new segment list is
    /// fully eligible. Callers should invoke this at `play` time and
    /// whenever an upstream ingest pipeline refreshes annotations.
    pub fn set_ad_segments(&mut self, segments: Vec<AdSegment>) {
        self.ad_segments = segments;
        self.skipped_ad_ids.clear();
    }

    /// Read-only view of the current episode's ad-break list. Used by
    /// the snapshot builder to surface segments on `EpisodeSummary`.
    #[must_use]
    pub fn ad_segments(&self) -> &[AdSegment] {
        &self.ad_segments
    }

    /// Flip the user's auto-skip toggle. Does not retroactively skip
    /// past a segment the playhead is currently inside — the next
    /// `Playing` report decides. Disabling does **not** clear the
    /// `skipped_ad_ids` set so a re-enable mid-session doesn't replay
    /// dismissed skips.
    pub fn set_auto_skip_ads(&mut self, enabled: bool) {
        self.auto_skip_ads = enabled;
    }

    /// Read-only view of the auto-skip toggle. Mirrored into the
    /// settings projection.
    #[must_use]
    pub fn auto_skip_ads(&self) -> bool {
        self.auto_skip_ads
    }

    /// Read-only view of the projected state. The FFI layer copies this
    /// into the `PodcastUpdate` snapshot.
    #[must_use]
    pub fn state(&self) -> &PlayerState {
        &self.state
    }

    /// Test/diagnostic snapshot of the active sleep-timer deadline.
    #[must_use]
    pub fn sleep_deadline(&self) -> Option<SystemTime> {
        self.sleep_deadline
    }

    /// Arm a sleep timer of `duration` from `now`. Subsequent
    /// `handle_audio_report` calls with a `now` past the deadline will
    /// emit `AudioCommand::Stop`.
    pub fn arm_sleep_timer(&mut self, duration: Duration, now: SystemTime) {
        self.sleep_deadline = Some(now + duration);
        self.sleep_timer_end_of_episode = false;
        self.state.sleep_timer_end_of_episode = false;
        self.refresh_sleep_remaining(now);
    }

    /// Arm the "stop at end of episode" sleep timer mode.
    pub fn arm_sleep_timer_end_of_episode(&mut self) {
        self.sleep_deadline = None;
        self.state.sleep_timer_remaining_secs = None;
        self.sleep_timer_end_of_episode = true;
        self.state.sleep_timer_end_of_episode = true;
    }

    /// Cancel any active sleep timer.
    pub fn cancel_sleep_timer(&mut self) {
        self.sleep_deadline = None;
        self.sleep_timer_end_of_episode = false;
        self.state.sleep_timer_remaining_secs = None;
        self.state.sleep_timer_end_of_episode = false;
    }

    /// Stage a `Load` request: stash the episode/podcast/url so future
    /// reports can correlate, and clear the previous error. The caller
    /// is expected to dispatch `AudioCommand::Load { url, position }`
    /// after staging.
    pub fn stage_load(
        &mut self,
        episode_id: impl Into<String>,
        podcast_id: Option<String>,
        url: impl Into<String>,
        position_secs: f64,
    ) {
        let url = url.into();
        self.state.episode_id = Some(episode_id.into());
        self.state.podcast_id = podcast_id;
        self.state.url = Some(url);
        self.state.position_secs = position_secs.max(0.0);
        self.state.is_playing = false;
        self.state.last_error = None;
        self.state.buffering_fraction = None;
        self.state.did_reach_natural_end = false;
        self.state.segment_end_secs = None;
        self.segment_end_reached = false;
    }

    /// Set or clear the current bounded segment end. The caller owns validating
    /// that the boundary is greater than the staged start position.
    pub fn set_segment_end_secs(&mut self, end_secs: Option<f64>) {
        self.state.segment_end_secs = end_secs;
        self.segment_end_reached = false;
    }

    /// Consume the one-shot bounded-segment terminal marker.
    pub(crate) fn take_segment_end_reached(&mut self) -> bool {
        let reached = self.segment_end_reached;
        self.segment_end_reached = false;
        reached
    }

    /// Project a `set_speed` action into state. Clamped to `0.5..=3.0`.
    pub fn set_speed(&mut self, speed: f32) {
        self.state.speed = speed.clamp(0.5, 3.0);
    }

    /// Project a seek action into state and return the applied target.
    ///
    /// Clamps to `0` and, when the player knows a positive duration, to the
    /// track duration. If duration is still unknown (`0.0`), upper-bound
    /// clamping waits for the capability's next report.
    pub fn seek_target(&mut self, position_secs: f64) -> f64 {
        let lower = position_secs.max(0.0);
        let target = if self.state.duration_secs > 0.0 {
            lower.min(self.state.duration_secs)
        } else {
            lower
        };
        self.state.position_secs = target;
        target
    }

    /// Project a `set_volume` action into state. Clamped to `0.0..=1.0`.
    pub fn set_volume(&mut self, volume: f32) {
        self.state.volume = volume.clamp(0.0, 1.0);
    }

    /// Mirror `auto_play_next` from the store. See [`Self::auto_play_next`].
    pub fn set_auto_play_next(&mut self, enabled: bool) {
        self.auto_play_next = enabled;
    }

    /// Mirror `auto_mark_played_at_end` from the store.
    pub fn set_auto_mark_played_at_end(&mut self, enabled: bool) {
        self.auto_mark_played_at_end = enabled;
    }
}

#[cfg(test)]
mod tests;
