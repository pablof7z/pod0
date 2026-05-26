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
mod tests {
    use super::*;

    #[test]
    fn fresh_store_has_default_skip_intervals() {
        let store = PodcastStore::new();
        assert!((store.skip_forward_secs() - 30.0).abs() < f64::EPSILON);
        assert!((store.skip_backward_secs() - 15.0).abs() < f64::EPSILON);
    }

    #[test]
    fn set_skip_intervals_updates_values() {
        let mut store = PodcastStore::new();
        store.set_skip_intervals(45.0, 10.0);
        assert!((store.skip_forward_secs() - 45.0).abs() < f64::EPSILON);
        assert!((store.skip_backward_secs() - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn set_skip_intervals_clamps_to_bounds() {
        let mut store = PodcastStore::new();
        store.set_skip_intervals(0.0, 200.0);
        assert!((store.skip_forward_secs() - 1.0).abs() < f64::EPSILON);
        assert!((store.skip_backward_secs() - 120.0).abs() < f64::EPSILON);
    }

    #[test]
    fn set_skip_intervals_same_value_is_noop() {
        let mut store = PodcastStore::new();
        // Writing defaults again must not change state
        store.set_skip_intervals(30.0, 15.0);
        assert!((store.skip_forward_secs() - 30.0).abs() < f64::EPSILON);
        assert!((store.skip_backward_secs() - 15.0).abs() < f64::EPSILON);
    }
}
