//! Playback-position and episode-state accessors for [`super::PodcastStore`].
//!
//! Extracted to keep `store/mod.rs` within the 500-line ceiling.
//!
//! Position writes are deliberately **not** persisted on every call —
//! `Playing` reports arrive at ≤4 Hz and flushing the full `podcasts.json` on
//! every tick would burn disk bandwidth and shorten flash life. Call
//! [`flush_positions`] at natural checkpoints (pause, stop, background,
//! periodic interval) instead.

use super::PodcastStore;

impl PodcastStore {
    /// Read the persisted playback position for an episode keyed by the string
    /// form of its UUID. Returns `None` when no episode with that id is found
    /// or when its position is at the start (`0.0`).
    ///
    /// Used by the snapshot projection so iOS can render a "Resume at X:XX"
    /// indicator without having to keep its own copy of the position. The Play
    /// path itself reads position directly via [`episode_playback_info`].
    pub fn position_for(&self, id_str: &str) -> Option<f64> {
        for episodes in self.episodes.values() {
            if let Some(ep) = episodes.iter().find(|e| e.id.0.to_string() == id_str) {
                return if ep.position_secs > 0.0 {
                    Some(ep.position_secs)
                } else {
                    None
                };
            }
        }
        None
    }

    /// Update an episode's playback position in memory. **Does not** persist;
    /// call [`flush_positions`] (or any other persisting mutation) to write
    /// through to disk.
    ///
    /// Returns `true` when the episode was found and updated, `false`
    /// otherwise. `Playing` reports arrive at ≤4 Hz (`AudioReport` D8); writing
    /// the entire `podcasts.json` on every tick would burn disk bandwidth and
    /// shorten flash life, so the writeback path stays in-memory and the FFI
    /// layer batches disk flushes on terminal events (pause / stop) and on a
    /// coarse interval. The "every durable concept has one canonical
    /// representation" rule (AGENTS.md) keeps position on `Episode.position_secs`
    /// rather than a parallel side-map.
    pub fn set_episode_position(&mut self, id_str: &str, position_secs: f64) -> bool {
        let pos = position_secs.max(0.0);
        for episodes in self.episodes.values_mut() {
            if let Some(ep) = episodes.iter_mut().find(|e| e.id.0.to_string() == id_str) {
                ep.position_secs = pos;
                return true;
            }
        }
        false
    }

    /// Mark an episode (by stringified `EpisodeId`) as listened. Returns
    /// `true` only when the flag actually flipped (unknown id and
    /// already-played both return `false`). Flushes to disk when bound.
    pub fn mark_episode_played(&mut self, id_str: &str) -> bool {
        for episodes in self.episodes.values_mut() {
            if let Some(ep) = episodes.iter_mut().find(|e| e.id.0.to_string() == id_str) {
                if ep.played {
                    return false;
                }
                ep.played = true;
                self.persist();
                return true;
            }
        }
        false
    }

    /// Return the tracked local path for delete-after-played, without mutating
    /// download state. The caller removes the file first and clears the mapping
    /// only when deletion succeeds or the path was already stale.
    pub fn auto_delete_download_candidate(
        &self,
        id_str: &str,
    ) -> Option<(podcast_core::EpisodeId, String)> {
        if !self.auto_delete_downloads_after_played() {
            return None;
        }
        self.download_delete_candidate(id_str)
    }

    /// Mark an episode (by stringified `EpisodeId`) as unlistened. Returns
    /// `true` only when the flag actually flipped. Flushes to disk when bound.
    pub fn mark_episode_unplayed(&mut self, id_str: &str) -> bool {
        for episodes in self.episodes.values_mut() {
            if let Some(ep) = episodes.iter_mut().find(|e| e.id.0.to_string() == id_str) {
                if !ep.played {
                    return false;
                }
                ep.played = false;
                self.persist();
                return true;
            }
        }
        false
    }

    /// Reset the playback position of an episode to zero and persist. Distinct
    /// from `mark_episode_unplayed` — the episode remains in the inbox but
    /// the "Continue Listening" resume point is cleared.
    pub fn reset_episode_progress(&mut self, id_str: &str) -> bool {
        for episodes in self.episodes.values_mut() {
            if let Some(ep) = episodes.iter_mut().find(|e| e.id.0.to_string() == id_str) {
                ep.position_secs = 0.0;
                self.flush_positions();
                return true;
            }
        }
        false
    }

    /// Set or toggle the `is_starred` flag for an episode.
    ///
    /// When `starred` is `Some(value)` the flag is set explicitly; when `None`
    /// the current value is flipped. Returns the new value, or `None` when the
    /// episode id is unknown. Persists immediately.
    pub fn set_episode_starred(&mut self, id_str: &str, starred: Option<bool>) -> Option<bool> {
        for episodes in self.episodes.values_mut() {
            if let Some(ep) = episodes.iter_mut().find(|e| e.id.0.to_string() == id_str) {
                let new_value = starred.unwrap_or(!ep.is_starred);
                ep.is_starred = new_value;
                self.persist();
                return Some(new_value);
            }
        }
        None
    }

    /// Force-flush the in-memory state to disk. Companion to
    /// [`set_episode_position`] — call when a natural checkpoint is reached
    /// (pause, stop, app background, periodic interval) so the in-memory
    /// position deltas survive a hard kill.
    ///
    /// Side-effect: snapshots the current `Episode.position_secs` for every
    /// known episode into the in-memory "last flushed" marker so subsequent
    /// `Playing` ticks can throttle off the on-disk state, not the previous
    /// in-memory tick. Silent no-op when no data dir has been bound (D6).
    pub fn flush_positions(&mut self) {
        // Take the snapshot of what we're about to persist before the rename
        // races, so the marker is consistent with what is now on disk.
        for episodes in self.episodes.values() {
            for ep in episodes {
                let key = ep.id.0.to_string();
                if ep.position_secs > 0.0 {
                    self.last_flushed_positions.insert(key, ep.position_secs);
                } else {
                    // A reset to 0 should clear the marker so the next
                    // forward-playing tick is treated as fresh.
                    self.last_flushed_positions.remove(&key);
                }
            }
        }
        self.persist();
    }

    /// Look up the most recently persisted position for an episode, or
    /// `None` when nothing has been flushed for it this session (and the
    /// initial hydration didn't seed a value). Used by the FFI writeback
    /// layer to decide whether the live playhead has drifted enough from
    /// the on-disk checkpoint to warrant another flush — the throttling
    /// MUST compare against the last *flushed* position rather than the
    /// previous tick's in-memory value, otherwise an uninterrupted stream
    /// of small ≤4 Hz `Playing` deltas never crosses the threshold.
    pub fn last_flushed_position(&self, id_str: &str) -> Option<f64> {
        self.last_flushed_positions.get(id_str).copied()
    }
}
