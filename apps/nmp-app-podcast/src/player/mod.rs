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
    /// Ordered list of queued episode ids ("Up Next"). The kernel
    /// owns this — the iOS shell only renders it from the snapshot
    /// projection and dispatches `enqueue`/`dequeue`/`clear_queue`/
    /// `play_next` actions to mutate it. Dedup is by id (an episode
    /// already present is not appended again).
    queue: Vec<String>,
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
}

impl PlayerActor {
    /// Construct an actor in the "idle, neutral defaults" state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: PlayerState::idle(),
            sleep_deadline: None,
            queue: Vec::new(),
            ad_segments: Vec::new(),
            auto_skip_ads: false,
            skipped_ad_ids: HashSet::new(),
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
        self.refresh_sleep_remaining(now);
    }

    /// Cancel any active sleep timer.
    pub fn cancel_sleep_timer(&mut self) {
        self.sleep_deadline = None;
        self.state.sleep_timer_remaining_secs = None;
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
    }

    /// Project a `set_speed` action into state. Clamped to `0.5..=3.0`.
    pub fn set_speed(&mut self, speed: f32) {
        self.state.speed = speed.clamp(0.5, 3.0);
    }

    /// Project a `set_volume` action into state. Clamped to `0.0..=1.0`.
    pub fn set_volume(&mut self, volume: f32) {
        self.state.volume = volume.clamp(0.0, 1.0);
    }

    // ---- Queue ("Up Next") -----------------------------------------------

    /// Snapshot of the current playback queue (ordered episode ids).
    /// Cheap clone; the snapshot builder copies this into
    /// `PodcastUpdate.queue`.
    #[must_use]
    pub fn queue(&self) -> &[String] {
        &self.queue
    }

    /// Append `episode_id` to the queue if it isn't already present.
    /// Dedup is by id only; this does not check against the currently
    /// playing episode (callers may legitimately want to queue the
    /// current episode for replay).
    pub fn enqueue(&mut self, episode_id: &str) {
        if !self.queue.iter().any(|id| id == episode_id) {
            self.queue.push(episode_id.to_owned());
        }
    }

    /// Remove the first occurrence of `episode_id` from the queue.
    /// Silent no-op when not present.
    pub fn dequeue(&mut self, episode_id: &str) {
        if let Some(idx) = self.queue.iter().position(|id| id == episode_id) {
            self.queue.remove(idx);
        }
    }

    /// Empty the queue.
    pub fn clear_queue(&mut self) {
        self.queue.clear();
    }

    /// Pop and return the front of the queue, or `None` when empty.
    /// Callers (the host-op handler) load + play the returned id.
    pub fn pop_next(&mut self) -> Option<String> {
        if self.queue.is_empty() {
            None
        } else {
            Some(self.queue.remove(0))
        }
    }

}

#[cfg(test)]
mod tests;
