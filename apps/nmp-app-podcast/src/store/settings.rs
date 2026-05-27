//! Settings accessors for [`super::PodcastStore`].
//!
//! Covers the onboarding-complete flag and the per-podcast auto-download
//! opt-in. Extracted to keep `store/mod.rs` within the 500-line ceiling.
//!
//! Persistence is handled by the parent module's `persist()` helper —
//! every mutator here calls `self.persist()` so changes survive restart.

use podcast_core::PodcastId;

use super::PodcastStore;

impl PodcastStore {
    /// Whether the user has finished the iOS onboarding flow. Read by the iOS
    /// shell from the `settings` snapshot to gate `OnboardingView`. Defaults
    /// to `false` for fresh installs.
    pub fn has_completed_onboarding(&self) -> bool {
        self.has_completed_onboarding
    }

    /// Update the onboarding-complete flag and flush to disk when a data dir
    /// is registered. Idempotent: writing the same value is a no-op for the
    /// disk file (the bytes are unchanged) and for the in-memory flag.
    pub fn set_onboarding_complete(&mut self, value: bool) {
        if self.has_completed_onboarding == value {
            return;
        }
        self.has_completed_onboarding = value;
        self.persist();
    }

    /// Set the auto-download opt-in flag for a podcast. Idempotent and
    /// silent when the podcast isn't subscribed (the flag will just
    /// hang around in the set; `unsubscribe` clears it). Flushes to
    /// disk when a data dir is bound so the preference survives
    /// app relaunches.
    pub fn set_auto_download(&mut self, podcast_id: PodcastId, enabled: bool) {
        let changed = if enabled {
            self.auto_download_enabled.insert(podcast_id)
        } else {
            self.auto_download_enabled.remove(&podcast_id)
        };
        if changed {
            self.persist();
        }
    }

    /// Read the auto-download opt-in flag for a podcast. Defaults to
    /// `false` for unknown / never-toggled podcasts.
    pub fn is_auto_download_enabled(&self, podcast_id: PodcastId) -> bool {
        self.auto_download_enabled.contains(&podcast_id)
    }

    /// Look up the auto-download flag by the string form of a podcast id.
    /// Helper for the FFI action handlers, which receive UUIDs as strings.
    pub fn is_auto_download_enabled_str(&self, id_str: &str) -> bool {
        match id_str.parse::<uuid::Uuid>() {
            Ok(uuid) => self.is_auto_download_enabled(PodcastId::new(uuid)),
            Err(_) => false,
        }
    }

    /// Whether to auto-advance to the next queued episode on `ItemEnd`.
    /// Default `true`. Controlled via `podcast.settings.set_auto_play_next`.
    pub fn auto_play_next(&self) -> bool {
        self.auto_play_next
    }

    /// Set the auto-play-next toggle and persist. Idempotent.
    pub fn set_auto_play_next(&mut self, value: bool) {
        if self.auto_play_next == value { return; }
        self.auto_play_next = value;
        self.persist();
    }

    /// Whether to mark the episode listened on natural `ItemEnd`.
    /// Default `true`.
    pub fn auto_mark_played_at_end(&self) -> bool {
        self.auto_mark_played_at_end
    }

    /// Set the auto-mark-played toggle and persist. Idempotent.
    pub fn set_auto_mark_played_at_end(&mut self, value: bool) {
        if self.auto_mark_played_at_end == value { return; }
        self.auto_mark_played_at_end = value;
        self.persist();
    }

    /// Raw action string for headphone double-tap gesture. Default `"skip_forward"`.
    pub fn headphone_double_tap_action(&self) -> &str {
        &self.headphone_double_tap_action
    }

    /// Raw action string for headphone triple-tap gesture. Default `"clip_now"`.
    pub fn headphone_triple_tap_action(&self) -> &str {
        &self.headphone_triple_tap_action
    }

    /// Update both headphone gesture action strings and persist. Idempotent.
    pub fn set_headphone_gesture_actions(&mut self, double_tap: String, triple_tap: String) {
        if self.headphone_double_tap_action == double_tap
            && self.headphone_triple_tap_action == triple_tap
        {
            return;
        }
        self.headphone_double_tap_action = double_tap;
        self.headphone_triple_tap_action = triple_tap;
        self.persist();
    }

    /// Skip-forward interval in seconds. Default 30.0; user-configurable via
    /// `podcast.settings.set_skip_intervals`.
    pub fn skip_forward_secs(&self) -> f64 {
        self.skip_forward_secs
    }

    /// Skip-backward interval in seconds. Default 15.0; user-configurable via
    /// `podcast.settings.set_skip_intervals`.
    pub fn skip_backward_secs(&self) -> f64 {
        self.skip_backward_secs
    }

    /// Update both skip intervals. Clamps each value to `[1.0, 120.0]`
    /// seconds and persists when either value changes.
    pub fn set_skip_intervals(&mut self, forward_secs: f64, backward_secs: f64) {
        let fwd = forward_secs.clamp(1.0, 120.0);
        let bwd = backward_secs.clamp(1.0, 120.0);
        if (self.skip_forward_secs - fwd).abs() < f64::EPSILON
            && (self.skip_backward_secs - bwd).abs() < f64::EPSILON
        {
            return;
        }
        self.skip_forward_secs = fwd;
        self.skip_backward_secs = bwd;
        self.persist();
    }
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
