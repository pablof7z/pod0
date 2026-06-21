//! Disk binding, hydration, and write-through persistence for `PodcastStore`.
//!
//! Split into sub-modules by responsibility:
//! - `load`    — cold-start hydration (`load_from_disk`)
//! - `persist` — snapshot serialisation (`to_persisted`, `persisted_settings`)

use std::path::{Path, PathBuf};

use super::persistence;
use super::PodcastStore;

mod load;
mod persist;

impl PodcastStore {
    /// Bind the store to a persistence directory and load any existing state.
    ///
    /// Replaces the current in-memory contents with whatever `podcasts.json`
    /// inside `dir` contains (or leaves them empty when the file is absent /
    /// corrupted). The directory is created if missing.
    ///
    /// Returns the number of podcasts loaded so the FFI wrapper can decide
    /// whether to bump `rev` and force iOS to re-poll the snapshot.
    ///
    /// Idempotent: calling twice with the same path is safe; calling with a
    /// new path rebinds and re-loads.
    pub fn set_data_dir(&mut self, dir: PathBuf) -> usize {
        // create_dir_all is a no-op when the directory already exists.
        let _ = std::fs::create_dir_all(&dir);
        self.data_dir = Some(dir.clone());
        self.load_from_disk()
    }

    /// Drain the queue snapshot that was hydrated by the most recent
    /// `set_data_dir` call. Returns an empty vec on all subsequent calls
    /// (and before any load). The FFI layer seeds `PlaybackQueue` from this
    /// value immediately after `set_data_dir` returns.
    pub fn take_loaded_queue(&mut self) -> Vec<crate::queue::QueuedPlaybackItem> {
        std::mem::take(&mut self.loaded_queue)
    }

    /// Flush the current in-memory state to `data_dir/podcasts.json`. Silent
    /// no-op when no data dir is set. Errors are intentionally swallowed
    /// (D6) — the in-memory store stays authoritative.
    pub(super) fn persist(&self) {
        let Some(dir) = self.data_dir.as_ref() else {
            return;
        };
        let mut payload = self.to_persisted();
        payload.queue = self
            .cached_queue
            .clone()
            .into_iter()
            .map(Into::into)
            .collect();
        let _ = persistence::save(dir, &payload);
    }

    /// Update the cached queue and flush to `data_dir/podcasts.json`. Called
    /// by the queue action handler after every mutation so the queue survives
    /// app restart. Silent no-op when no data dir is set (D6).
    pub(crate) fn persist_with_queue(&mut self, queue_items: &[crate::queue::QueuedPlaybackItem]) {
        self.cached_queue = queue_items.to_vec();
        self.persist();
    }

    /// Accessor for the currently-bound data dir, or `None` before
    /// `set_data_dir`. Read by the host-op handler's relay-edit arm to
    /// locate the relay-config sidecar (`relay_config::save_relay_config`).
    pub(crate) fn data_dir(&self) -> Option<&Path> {
        self.data_dir.as_deref()
    }
}
