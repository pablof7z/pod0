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
mod owned_ext;
mod persistence;
pub mod podcast_keys;
#[cfg(test)]
mod tests;
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
    data_dir: Option<PathBuf>,
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
            data_dir: None,
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
        self.podcasts.len()
    }

    /// Flush the current in-memory state to `data_dir/podcasts.json`. Silent
    /// no-op when no data dir is set. Errors are intentionally swallowed
    /// (D6) — the in-memory store stays authoritative.
    pub(super) fn persist(&self) {
        let Some(dir) = self.data_dir.as_ref() else { return; };
        let payload = self.to_persisted();
        let _ = persistence::save(dir, &payload);
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
            settings: PersistedSettings { auto_skip_ads_enabled: self.auto_skip_ads_enabled },
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

    /// Whether the user has finished iOS onboarding. The iOS shell reads this
    /// from the `settings` snapshot to gate `OnboardingView`. Defaults to
    /// `false` for fresh installs.
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

    /// Read-only access to the on-disk path side-map. Used by the
    /// auto-download policy helper so a "freshly discovered" episode
    /// already known to be on disk is not re-queued.
    pub fn local_paths(&self) -> &HashMap<EpisodeId, String> {
        &self.local_paths
    }

    // ── Agent memory (feature #33) ────────────────────────────────────────

    /// Upsert a memory fact keyed on `key`. When a fact with the same key
    /// already exists, only the value and source change — the original
    /// `created_at` and `id` are preserved so the UI sees stable identity
    /// across edits.
    ///
    /// `source` is taken verbatim; the action handler is responsible for
    /// defaulting it (typically to `"user"`).
    pub fn set_memory_fact(&mut self, key: String, value: String, source: String, now_unix: i64) {
        let fact = match self.memory_facts.get(&key) {
            Some(existing) => MemoryFact {
                id: existing.id.clone(),
                key: existing.key.clone(),
                value,
                source,
                created_at: existing.created_at,
            },
            None => MemoryFact {
                id: key.clone(),
                key: key.clone(),
                value,
                source,
                created_at: now_unix,
            },
        };
        self.memory_facts.insert(key, fact);
        self.persist();
    }

    /// Delete a memory fact by key. Returns `true` when a row was removed
    /// so the caller can decide whether to bump `rev`.
    pub fn remove_memory_fact(&mut self, key: &str) -> bool {
        let removed = self.memory_facts.remove(key).is_some();
        if removed {
            self.persist();
        }
        removed
    }

    /// Wipe the entire memory bag. Returns the number of facts that were
    /// removed so the caller can decide whether to bump `rev`.
    pub fn clear_memory(&mut self) -> usize {
        let n = self.memory_facts.len();
        if n > 0 {
            self.memory_facts.clear();
            self.persist();
        }
        n
    }

    /// Snapshot of every memory fact, sorted by `key` so the iOS list is
    /// stable across re-renders without a client-side sort.
    pub fn all_memory_facts(&self) -> Vec<MemoryFact> {
        let mut facts: Vec<MemoryFact> = self.memory_facts.values().cloned().collect();
        facts.sort_by(|a, b| a.key.cmp(&b.key));
        facts
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
