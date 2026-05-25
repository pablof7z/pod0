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

use std::time::{Duration, SystemTime};

use crate::capability::{AudioCommand, AudioReport};

mod state;
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
}

impl PlayerActor {
    /// Construct an actor in the "idle, neutral defaults" state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: PlayerState::idle(),
            sleep_deadline: None,
        }
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

    /// Project a `set_speed` action into state. Clamped to `0.5..=2.0`.
    pub fn set_speed(&mut self, speed: f32) {
        self.state.speed = speed.clamp(0.5, 2.0);
    }

    /// Project a `set_volume` action into state. Clamped to `0.0..=1.0`.
    pub fn set_volume(&mut self, volume: f32) {
        self.state.volume = volume.clamp(0.0, 1.0);
    }

    /// Handle an inbound report from the iOS audio capability.
    ///
    /// Returns the [`AudioCommand`] the FFI layer should send back
    /// through the capability, if any. Today the only follow-up command
    /// is `Stop` on sleep-timer expiry; future capability features
    /// (auto-advance, chapter snap-back) will land here in M3.B.
    pub fn handle_audio_report(
        &mut self,
        report: AudioReport,
        now: SystemTime,
    ) -> Option<AudioCommand> {
        match report {
            AudioReport::Playing {
                url,
                position_secs,
                duration_secs,
            } => self.on_playing(url, position_secs, duration_secs, now),
            AudioReport::Paused { url, position_secs } => {
                self.on_paused(&url, position_secs, now);
                None
            }
            AudioReport::Stopped => {
                self.on_stopped();
                None
            }
            AudioReport::Failed { url, error } => {
                self.on_failed(&url, error);
                None
            }
            AudioReport::BufferingProgress { fraction } => {
                self.state.buffering_fraction = Some(fraction.clamp(0.0, 1.0));
                self.refresh_sleep_remaining(now);
                None
            }
            AudioReport::SleepTimerFired => {
                // The iOS-side timer fired (e.g. for lock-screen fade).
                // Rust still owns the decision — emit `Stop` and clear
                // the deadline so a subsequent `Play` doesn't trip it
                // again immediately. (D9.)
                self.sleep_deadline = None;
                self.state.sleep_timer_remaining_secs = None;
                Some(AudioCommand::Stop)
            }
        }
    }

    // ---- Per-variant report handlers -------------------------------------

    fn on_playing(
        &mut self,
        url: String,
        position_secs: f64,
        duration_secs: f64,
        now: SystemTime,
    ) -> Option<AudioCommand> {
        self.state.url = Some(url);
        self.state.position_secs = position_secs.max(0.0);
        if duration_secs > 0.0 {
            self.state.duration_secs = duration_secs;
        }
        self.state.is_playing = true;
        self.state.buffering_fraction = None;
        self.state.last_error = None;

        // D9: check the authoritative sleep-timer deadline here, not
        // on the iOS side. If we've elapsed, ask iOS to stop and clear
        // the deadline so a Play that arrives between the deadline and
        // the eventual Stop report doesn't trip the same expiry twice.
        if let Some(deadline) = self.sleep_deadline {
            if now >= deadline {
                self.sleep_deadline = None;
                self.state.sleep_timer_remaining_secs = None;
                return Some(AudioCommand::Stop);
            }
        }
        self.refresh_sleep_remaining(now);
        None
    }

    fn on_paused(&mut self, url: &str, position_secs: f64, now: SystemTime) {
        if !url.is_empty() {
            self.state.url = Some(url.to_owned());
        }
        self.state.position_secs = position_secs.max(0.0);
        self.state.is_playing = false;
        self.state.buffering_fraction = None;
        self.refresh_sleep_remaining(now);
    }

    fn on_stopped(&mut self) {
        self.state.is_playing = false;
        self.state.buffering_fraction = None;
        // Clear the timer on a hard stop so re-arming is required.
        self.sleep_deadline = None;
        self.state.sleep_timer_remaining_secs = None;
    }

    fn on_failed(&mut self, url: &str, error: String) {
        if !url.is_empty() {
            self.state.url = Some(url.to_owned());
        }
        self.state.is_playing = false;
        self.state.buffering_fraction = None;
        self.state.last_error = Some(error);
    }

    /// Recompute the visible sleep-timer countdown from the stored
    /// deadline and `now`. Hides the field when no timer is armed or
    /// when the deadline has elapsed (the expiry handler emits `Stop`).
    fn refresh_sleep_remaining(&mut self, now: SystemTime) {
        let Some(deadline) = self.sleep_deadline else {
            self.state.sleep_timer_remaining_secs = None;
            return;
        };
        match deadline.duration_since(now) {
            Ok(remaining) => {
                self.state.sleep_timer_remaining_secs = Some(remaining.as_secs());
            }
            Err(_) => {
                // Elapsed — surface a zero so the UI can show "0:00"
                // for a frame before the expiry handler clears it on
                // the next report.
                self.state.sleep_timer_remaining_secs = Some(0);
            }
        }
    }
}

#[cfg(test)]
mod tests;
