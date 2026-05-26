//! Podcast library store.
//!
//! Holds the set of subscribed podcasts and their episodes. Keyed by `PodcastId`
//! so lookups are O(1); the store is wrapped in `Arc<Mutex<PodcastStore>>` and
//! shared between the `PodcastHandle` (snapshot reader) and the
//! `PodcastHostOpHandler` (writer). All writes happen on the actor thread;
//! reads happen on the iOS main thread via `nmp_app_podcast_snapshot`.
//!
//! ## Persistence
//!
//! When [`PodcastStore::set_data_dir`] has been called the store mirrors every
//! mutation (`subscribe` / `unsubscribe` / `update_refresh_metadata`) to a
//! single `podcasts.json` file inside that directory. Reads stay purely
//! in-memory; the disk file is a write-through cache so the library survives
//! app restarts.
//!
//! D6: persistence failures degrade silently — the in-memory store remains
//! authoritative and the next mutation will try to write again.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
#[cfg(test)]
use std::path::Path;

use podcast_core::{Episode, EpisodeId, Podcast, PodcastId};

mod ad_segments;
mod chapters;
pub mod auto_download;
mod memory;
mod owned_ext;
mod playback;
mod persistence;
pub mod podcast_keys;
mod settings;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_ext;
mod transcripts;

use crate::ffi::projections::MemoryFact;
use crate::player::AdSegment;
pub use auto_download::episodes_to_auto_download;
pub use podcast_keys::PodcastKeyStore;
use persistence::{PersistedPodcast, PersistedSettings, PersistedStore, PERSIST_SCHEMA_VERSION};

/// Backing store for subscribed podcasts and their episode lists.
///
/// Mutations flush to `data_dir/podcasts.json` (atomic temp+rename) when a
/// data dir has been registered via [`Self::set_data_dir`]. Without a data
/// dir the store stays in memory — useful for unit tests and the very first
/// run before iOS calls `nmp_app_podcast_set_data_dir`.
pub struct PodcastStore {
    pub(super) podcasts: HashMap<PodcastId, Podcast>,
    pub(super) episodes: HashMap<PodcastId, Vec<Episode>>,
    /// Per-episode on-disk path for downloaded enclosures. Populated when an
    /// iOS `DownloadCapability` reports `Completed`; cleared by
    /// [`PodcastStore::clear_local_path`] when the user deletes the file.
    ///
    /// Lives in a side-map so refreshing a feed, which replaces the episode
    /// list wholesale, does not wipe download state.
    local_paths: HashMap<EpisodeId, String>,
    /// Plain-text transcripts keyed by the string form of `EpisodeId`.
    transcripts: HashMap<String, String>,
    /// Last position (seconds) committed to disk for each episode, keyed by
    /// the string form of `EpisodeId`. Used by the writeback layer to decide
    /// whether the live playhead has drifted enough from the on-disk
    /// checkpoint to warrant another `persist()`. Cleared on `set_data_dir`
    /// since a freshly-bound store hasn't flushed anything yet — the
    /// hydrated values from disk are themselves the most-recent checkpoint.
    /// Not persisted: this is a runtime throttling marker, not durable state.
    last_flushed_positions: HashMap<String, f64>,
    /// Whether the user has finished the iOS onboarding flow. Surfaced via
    /// the `settings` snapshot projection so the iOS shell can decide
    /// whether to present `OnboardingView`. Mirrored to disk under the same
    /// `podcasts.json` envelope as the library so the flag survives restart.
    has_completed_onboarding: bool,
    /// Podcasts the user has opted into auto-download for.
    ///
    /// Membership is the policy: present ⇒ `handle_refresh` will queue
    /// freshly-discovered episodes via the download capability; absent ⇒
    /// new episodes are surfaced in the snapshot but not downloaded.
    /// Cleared by `unsubscribe` so a later re-subscribe starts fresh.
    auto_download_enabled: HashSet<PodcastId>,
    /// Durable agent-memory bag (feature #33). Keyed on `MemoryFact.key`
    /// so writes upsert and the snapshot can render a deduped list. Lives
    /// alongside `podcasts` in `podcasts.json` so both projections share
    /// one persistence pass.
    memory_facts: HashMap<String, MemoryFact>,
    /// Per-episode ad-break intervals keyed by the string form of
    /// `EpisodeId`. See [`mod@ad_segments`] for the accessor surface.
    pub(super) ad_segments: HashMap<String, Vec<AdSegment>>,
    /// User toggle: auto-skip ads when the playhead enters one.
    pub(super) auto_skip_ads_enabled: bool,
    /// Skip-forward interval (seconds). Default 30.0; user-configurable.
    pub(super) skip_forward_secs: f64,
    /// Skip-backward interval (seconds). Default 15.0; user-configurable.
    pub(super) skip_backward_secs: f64,
    data_dir: Option<PathBuf>,
    /// Episode ids loaded from disk during `set_data_dir`. Drained exactly
    /// once by `take_loaded_queue`; the FFI layer seeds the shared
    /// `PlaybackQueue` from this value after load completes.
    loaded_queue: Vec<String>,
    /// Current "Up Next" queue, mirrored here so that ordinary `persist()`
    /// calls (triggered by subscription changes, settings tweaks, etc.) write
    /// the real queue rather than an empty slice.  Updated by every
    /// `persist_with_queue` call and seeded from disk on `load_from_disk`.
    cached_queue: Vec<String>,
}

impl PodcastStore {
    pub fn new() -> Self {
        Self {
            podcasts: HashMap::new(),
            episodes: HashMap::new(),
            local_paths: HashMap::new(),
            transcripts: HashMap::new(),
            last_flushed_positions: HashMap::new(),
            has_completed_onboarding: false,
            auto_download_enabled: HashSet::new(),
            memory_facts: HashMap::new(),
            ad_segments: HashMap::new(),
            auto_skip_ads_enabled: false,
            skip_forward_secs: 30.0,
            skip_backward_secs: 15.0,
            data_dir: None,
            loaded_queue: Vec::new(),
            cached_queue: Vec::new(),
        }
    }

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

    /// Reload from `data_dir/podcasts.json`. Returns the number of podcasts
    /// hydrated. Silent no-op when no data dir is set or the file is missing.
    fn load_from_disk(&mut self) -> usize {
        let Some(dir) = self.data_dir.as_ref() else { return 0; };
        let loaded = match persistence::load(dir) {
            Ok(Some(payload)) => payload,
            Ok(None) => return 0,
            Err(_) => return 0, // D6 — corrupted file ⇒ start fresh on next write
        };
        self.podcasts.clear();
        self.episodes.clear();
        self.local_paths.clear();
        self.transcripts.clear();
        // Hydrated episode positions are themselves the most-recent flushed
        // checkpoint: seed the throttling marker so the writeback layer
        // doesn't immediately re-flush on the next `Playing` tick.
        self.last_flushed_positions.clear();
        self.auto_download_enabled.clear();
        self.memory_facts.clear();
        self.ad_segments.clear();
        for row in loaded.podcasts {
            let id = row.podcast.id;
            for ep in &row.episodes {
                if ep.position_secs > 0.0 {
                    self.last_flushed_positions
                        .insert(ep.id.0.to_string(), ep.position_secs);
                }
            }
            self.podcasts.insert(id, row.podcast);
            self.episodes.insert(id, row.episodes);
            if row.auto_download {
                self.auto_download_enabled.insert(id);
            }
        }
        // Settings are stored in the same envelope so onboarding completion
        // survives restart without a second file. `serde(default)` keeps
        // older saved files (predating the field) loading cleanly.
        self.has_completed_onboarding = loaded.has_completed_onboarding;
        for fact in loaded.memory_facts {
            self.memory_facts.insert(fact.key.clone(), fact);
        }
        for (ep_id, segs) in loaded.ad_segments {
            self.ad_segments.insert(ep_id, segs);
        }
        self.auto_skip_ads_enabled = loaded.settings.auto_skip_ads_enabled;
        // On-disk value of 0.0 means "field absent in old file" — replace
        // with the semantic default so the UI gets a usable value.
        self.skip_forward_secs = if loaded.settings.skip_forward_secs > 0.0 {
            loaded.settings.skip_forward_secs
        } else {
            30.0
        };
        self.skip_backward_secs = if loaded.settings.skip_backward_secs > 0.0 {
            loaded.settings.skip_backward_secs
        } else {
            15.0
        };
        self.cached_queue = loaded.queue.clone();
        self.loaded_queue = loaded.queue;
        self.podcasts.len()
    }

    /// Drain the queue snapshot that was hydrated by the most recent
    /// `set_data_dir` call. Returns an empty vec on all subsequent calls
    /// (and before any load). The FFI layer seeds `PlaybackQueue` from this
    /// value immediately after `set_data_dir` returns.
    pub fn take_loaded_queue(&mut self) -> Vec<String> {
        std::mem::take(&mut self.loaded_queue)
    }

    /// Flush the current in-memory state to `data_dir/podcasts.json`. Silent
    /// no-op when no data dir is set. Errors are intentionally swallowed
    /// (D6) — the in-memory store stays authoritative.
    pub(super) fn persist(&self) {
        let Some(dir) = self.data_dir.as_ref() else { return; };
        let mut payload = self.to_persisted();
        payload.queue = self.cached_queue.clone();
        let _ = persistence::save(dir, &payload);
    }

    /// Update the cached queue and flush to `data_dir/podcasts.json`. Called
    /// by the queue action handler after every mutation so the queue survives
    /// app restart. Silent no-op when no data dir is set (D6).
    pub(crate) fn persist_with_queue(&mut self, queue_items: &[String]) {
        self.cached_queue = queue_items.to_vec();
        self.persist();
    }

    fn to_persisted(&self) -> PersistedStore {
        let mut rows: Vec<PersistedPodcast> = self
            .podcasts
            .iter()
            .map(|(id, podcast)| PersistedPodcast {
                podcast: podcast.clone(),
                episodes: self.episodes.get(id).cloned().unwrap_or_default(),
                auto_download: self.auto_download_enabled.contains(id),
            })
            .collect();
        // Stable order so two consecutive saves produce identical bytes —
        // helps when diffing on-disk state during debugging.
        rows.sort_by(|a, b| a.podcast.id.0.cmp(&b.podcast.id.0));
        let mut facts: Vec<MemoryFact> = self.memory_facts.values().cloned().collect();
        // Same stable-order rationale as podcasts: keep saves byte-stable.
        facts.sort_by(|a, b| a.key.cmp(&b.key));
        let ad_segments: std::collections::BTreeMap<String, Vec<AdSegment>> = self
            .ad_segments
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        PersistedStore {
            schema_version: PERSIST_SCHEMA_VERSION,
            podcasts: rows,
            has_completed_onboarding: self.has_completed_onboarding,
            memory_facts: facts,
            ad_segments: ad_segments.into_iter().collect(),
            settings: PersistedSettings {
                auto_skip_ads_enabled: self.auto_skip_ads_enabled,
                skip_forward_secs: self.skip_forward_secs,
                skip_backward_secs: self.skip_backward_secs,
            },
            queue: Vec::new(), // filled by persist() from self.cached_queue after return
        }
    }

    /// Add or replace a podcast and its episode list, flushing to disk if
    /// a data dir is registered.
    ///
    /// Idempotent: re-subscribing to the same feed URL replaces the existing
    /// record so a "refresh" can use the same code path as "subscribe".
    pub fn subscribe(&mut self, podcast: Podcast, episodes: Vec<Episode>) {
        let id = podcast.id;
        self.podcasts.insert(id, podcast);
        self.episodes.insert(id, episodes);
        self.persist();
    }

    /// Iterate over all podcasts and their episode slices.
    pub fn all_podcasts(&self) -> Vec<(&Podcast, &[Episode])> {
        let mut result = Vec::with_capacity(self.podcasts.len());
        for (id, podcast) in &self.podcasts {
            let eps = self.episodes.get(id).map(Vec::as_slice).unwrap_or(&[]);
            result.push((podcast, eps));
        }
        result
    }

    pub fn podcast_count(&self) -> usize {
        self.podcasts.len()
    }

    pub fn episodes_for(&self, podcast_id: PodcastId) -> &[Episode] {
        self.episodes.get(&podcast_id).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn podcast(&self, podcast_id: PodcastId) -> Option<&Podcast> {
        self.podcasts.get(&podcast_id)
    }

    /// Remove a podcast and all its episodes, flushing to disk if a data dir
    /// is registered. Silent no-op when not found.
    ///
    /// Also clears the per-podcast auto-download flag so a later
    /// re-subscribe starts from "off" — otherwise stale policy from
    /// a previous lifetime of the show would silently keep firing.
    pub fn unsubscribe(&mut self, podcast_id: PodcastId) {
        let removed_p = self.podcasts.remove(&podcast_id).is_some();
        let removed_e = self.episodes.remove(&podcast_id).is_some();
        let removed_a = self.auto_download_enabled.remove(&podcast_id);
        if removed_p || removed_e || removed_a {
            self.persist();
        }
    }

    /// Look up a podcast by the string form of its UUID.
    pub fn podcast_by_id_str(&self, id_str: &str) -> Option<&Podcast> {
        self.podcasts.values().find(|p| p.id.0.to_string() == id_str)
    }

    /// Return `true` when a podcast with the given RSS feed URL is already
    /// subscribed. Used to reject duplicate `subscribe` actions before
    /// the HTTP fetch fires.
    pub fn has_feed_url(&self, url: &url::Url) -> bool {
        self.podcasts.values().any(|p| p.feed_url.as_ref() == Some(url))
    }

    /// Return `(id, feed_url, etag, last_modified)` for every podcast that has
    /// an RSS feed URL. Used by `refresh_all`.
    pub fn all_feed_infos(&self) -> Vec<(PodcastId, url::Url, Option<String>, Option<String>)> {
        self.podcasts
            .values()
            .filter_map(|p| {
                p.feed_url
                    .clone()
                    .map(|url| (p.id, url, p.etag.clone(), p.last_modified.clone()))
            })
            .collect()
    }

    /// Patch refresh metadata (etag/last-modified/timestamp) after a successful
    /// feed refresh without replacing the entire podcast record. Flushes to
    /// disk when a data dir is registered.
    pub fn update_refresh_metadata(
        &mut self,
        podcast_id: PodcastId,
        etag: Option<String>,
        last_modified: Option<String>,
    ) {
        if let Some(podcast) = self.podcasts.get_mut(&podcast_id) {
            podcast.etag = etag;
            podcast.last_modified = last_modified;
            podcast.last_refreshed_at = Some(chrono::Utc::now());
            self.persist();
        }
    }

    /// Find episode playback info by the string form of its `EpisodeId` UUID.
    ///
    /// Returns `(podcast_id_str, enclosure_url, position_secs)` or `None` when
    /// no episode with that id is found. Compares by converting stored UUIDs to
    /// their hyphenated string form — same format used in `EpisodeSummary.id`.
    pub fn episode_playback_info(&self, id_str: &str) -> Option<(String, String, f64)> {
        for (podcast_id, episodes) in &self.episodes {
            if let Some(ep) = episodes.iter().find(|e| e.id.0.to_string() == id_str) {
                return Some((
                    podcast_id.0.to_string(),
                    ep.enclosure_url.to_string(),
                    ep.position_secs,
                ));
            }
        }
        None
    }

    /// Resolve an episode UUID string to `(episode_title, podcast_title,
    /// duration_secs)`. Used by `ClipHandler` to (a) stamp the create-time
    /// titles into the `ClipRecord` so an unsubscribed-episode clip can
    /// still render, and (b) clamp the AutoSnip window to the episode
    /// duration when known.
    pub fn episode_titles_and_duration(
        &self,
        id_str: &str,
    ) -> Option<(String, String, Option<f64>)> {
        for (podcast_id, episodes) in &self.episodes {
            if let Some(ep) = episodes.iter().find(|e| e.id.0.to_string() == id_str) {
                let pod = self.podcasts.get(podcast_id)?;
                return Some((ep.title.clone(), pod.title.clone(), ep.duration_secs));
            }
        }
        None
    }

    /// Resolve an episode UUID string to its `EpisodeId` + enclosure URL.
    ///
    /// Used by the download handler to translate a wire-format episode id into
    /// the typed key and the URL the download capability should fetch.
    pub fn episode_enclosure_url(&self, id_str: &str) -> Option<(EpisodeId, String)> {
        for episodes in self.episodes.values() {
            if let Some(ep) = episodes.iter().find(|e| e.id.0.to_string() == id_str) {
                return Some((ep.id, ep.enclosure_url.to_string()));
            }
        }
        None
    }

    /// Record the on-disk path of a successfully downloaded enclosure.
    pub fn set_local_path(&mut self, episode_id: EpisodeId, path: String) {
        self.local_paths.insert(episode_id, path);
    }

    /// Look up the on-disk path of a downloaded enclosure, if any.
    pub fn local_path_for(&self, episode_id: &EpisodeId) -> Option<&str> {
        self.local_paths.get(episode_id).map(String::as_str)
    }

    /// Remove the local-path mapping for an episode and return the path that
    /// was previously stored so the caller can remove the file.
    pub fn clear_local_path(&mut self, episode_id: &EpisodeId) -> Option<String> {
        self.local_paths.remove(episode_id)
    }

    /// Read-only access to the on-disk path side-map. Used by the
    /// auto-download policy helper so a "freshly discovered" episode
    /// already known to be on disk is not re-queued.
    pub fn local_paths(&self) -> &HashMap<EpisodeId, String> {
        &self.local_paths
    }

    /// Test-only accessor for the currently-bound data dir.
    #[cfg(test)]
    pub(crate) fn data_dir(&self) -> Option<&Path> {
        self.data_dir.as_deref()
    }
}

impl Default for PodcastStore {
    fn default() -> Self {
        Self::new()
    }
}
