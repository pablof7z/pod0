//! `PlayerActor` audio-report handler.
//!
//! Extracted from `player/mod.rs` to keep that file within the 300-line
//! soft limit. All methods here are pure state mutations over [`PlayerActor`]
//! fields; the public entry point is [`PlayerActor::handle_audio_report`].

use std::time::SystemTime;

use crate::capability::{AudioCommand, AudioReport};

use super::ad_segments::contains as ad_segment_contains;
use super::PlayerActor;

impl PlayerActor {
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
            AudioReport::ItemEnd { .. } => {
                // Natural play-to-completion: set the natural-end flag so
                // the snapshot surface and M1.3 business logic can
                // distinguish this from a user-initiated stop. The flag is
                // cleared in `stage_load` so a subsequent play starts fresh.
                // `on_stopped` handles the remaining housekeeping (clear
                // timer, clear skipped ads, set is_playing = false).
                self.state.did_reach_natural_end = true;
                self.on_stopped();
                None
            }
        }
    }

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
        // Auto ad-skip after the sleep-timer check so a sleep expiry
        // takes precedence over a seek (don't seek past an ad just to
        // immediately stop).
        if let Some(cmd) = self.maybe_skip_ad(self.state.position_secs) {
            return Some(cmd);
        }
        None
    }

    /// If `auto_skip_ads` is on and `position_secs` falls inside an
    /// unseen `AdSegment`, mark the id as skipped and emit a `Seek` to
    /// its `end_secs`. Returns `None` otherwise. Pure: no state read
    /// beyond the actor's own fields, no clock reference.
    fn maybe_skip_ad(&mut self, position_secs: f64) -> Option<AudioCommand> {
        if !self.auto_skip_ads || self.ad_segments.is_empty() {
            return None;
        }
        let segment = self
            .ad_segments
            .iter()
            .find(|s| ad_segment_contains(s, position_secs) && !self.skipped_ad_ids.contains(&s.id))?;
        let target = segment.end_secs;
        self.skipped_ad_ids.insert(segment.id);
        Some(AudioCommand::seek(target))
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
        // End-of-session: forget which ads we already auto-skipped so
        // a re-listen of the same episode starts with a clean slate.
        self.skipped_ad_ids.clear();
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
    pub(super) fn refresh_sleep_remaining(&mut self, now: SystemTime) {
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
