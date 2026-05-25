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

use std::collections::HashMap;
use std::path::PathBuf;
#[cfg(test)]
use std::path::Path;

use podcast_core::{Episode, EpisodeId, Podcast, PodcastId};

mod persistence;

use persistence::{PersistedPodcast, PersistedStore, PERSIST_SCHEMA_VERSION};

/// Backing store for subscribed podcasts and their episode lists.
///
/// Mutations flush to `data_dir/podcasts.json` (atomic temp+rename) when a
/// data dir has been registered via [`Self::set_data_dir`]. Without a data
/// dir the store stays in memory — useful for unit tests and the very first
/// run before iOS calls `nmp_app_podcast_set_data_dir`.
pub struct PodcastStore {
    podcasts: HashMap<PodcastId, Podcast>,
    episodes: HashMap<PodcastId, Vec<Episode>>,
    /// Per-episode on-disk path for downloaded enclosures. Populated when an
    /// iOS `DownloadCapability` reports `Completed`; cleared by
    /// [`PodcastStore::clear_local_path`] when the user deletes the file.
    ///
    /// Lives in a side-map so refreshing a feed, which replaces the episode
    /// list wholesale, does not wipe download state.
    local_paths: HashMap<EpisodeId, String>,
    data_dir: Option<PathBuf>,
}

impl PodcastStore {
    pub fn new() -> Self {
        Self {
            podcasts: HashMap::new(),
            episodes: HashMap::new(),
            local_paths: HashMap::new(),
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
        for row in loaded.podcasts {
            let id = row.podcast.id;
            self.podcasts.insert(id, row.podcast);
            self.episodes.insert(id, row.episodes);
        }
        self.podcasts.len()
    }

    /// Flush the current in-memory state to `data_dir/podcasts.json`. Silent
    /// no-op when no data dir is set. Errors are intentionally swallowed
    /// (D6) — the in-memory store stays authoritative.
    fn persist(&self) {
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
            })
            .collect();
        // Stable order so two consecutive saves produce identical bytes —
        // helps when diffing on-disk state during debugging.
        rows.sort_by(|a, b| a.podcast.id.0.cmp(&b.podcast.id.0));
        PersistedStore {
            schema_version: PERSIST_SCHEMA_VERSION,
            podcasts: rows,
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
    pub fn unsubscribe(&mut self, podcast_id: PodcastId) {
        let removed_p = self.podcasts.remove(&podcast_id).is_some();
        let removed_e = self.episodes.remove(&podcast_id).is_some();
        if removed_p || removed_e {
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

#[cfg(test)]
mod tests {
    use super::*;
    use podcast_core::{Episode, Podcast, PodcastId};
    use std::sync::atomic::{AtomicU64, Ordering};
    use uuid::Uuid;

    /// RAII tempdir for store integration tests. Same pattern as
    /// `persistence::tests::TempDir`; duplicated here so the two test modules
    /// can be reordered independently.
    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new() -> Self {
            static SEQ: AtomicU64 = AtomicU64::new(0);
            let n = SEQ.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "nmp-podcast-store-{}-{}",
                std::process::id(),
                n,
            ));
            std::fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn make_podcast(title: &str) -> Podcast {
        Podcast::new(title)
    }

    fn make_episode(podcast_id: PodcastId, title: &str) -> Episode {
        Episode::new(
            podcast_id,
            format!("guid-{}", Uuid::new_v4()),
            title,
            url::Url::parse("https://example.com/audio.mp3").unwrap(),
            chrono::Utc::now(),
        )
    }

    #[test]
    fn subscribe_and_retrieve() {
        let mut store = PodcastStore::new();
        let podcast = make_podcast("Test Show");
        let id = podcast.id;
        store.subscribe(podcast, vec![]);
        assert_eq!(store.podcast_count(), 1);
        assert!(store.podcast(id).is_some());
    }

    #[test]
    fn all_podcasts_returns_all() {
        let mut store = PodcastStore::new();
        store.subscribe(make_podcast("Show A"), vec![]);
        store.subscribe(make_podcast("Show B"), vec![]);
        assert_eq!(store.all_podcasts().len(), 2);
    }

    #[test]
    fn resubscribe_replaces_existing() {
        let mut store = PodcastStore::new();
        let p1 = make_podcast("Original Title");
        let id = p1.id;
        store.subscribe(p1, vec![]);

        let mut p2 = make_podcast("Updated Title");
        p2.id = id; // same id — should replace
        store.subscribe(p2, vec![]);
        assert_eq!(store.podcast_count(), 1);
        assert_eq!(store.podcast(id).map(|p| p.title.as_str()), Some("Updated Title"));
    }

    #[test]
    fn set_and_get_local_path() {
        let mut store = PodcastStore::new();
        let ep_id = EpisodeId::generate();
        assert!(store.local_path_for(&ep_id).is_none());
        store.set_local_path(ep_id, "/tmp/ep.mp3".into());
        assert_eq!(store.local_path_for(&ep_id), Some("/tmp/ep.mp3"));
    }

    #[test]
    fn clear_local_path_returns_previous_and_unsets() {
        let mut store = PodcastStore::new();
        let ep_id = EpisodeId::generate();
        store.set_local_path(ep_id, "/tmp/ep.mp3".into());
        let prev = store.clear_local_path(&ep_id);
        assert_eq!(prev.as_deref(), Some("/tmp/ep.mp3"));
        assert!(store.local_path_for(&ep_id).is_none());
        assert!(store.clear_local_path(&ep_id).is_none());
    }

    // ── Persistence integration tests ────────────────────────────────────

    #[test]
    fn set_data_dir_on_empty_dir_returns_zero() {
        let dir = TempDir::new();
        let mut store = PodcastStore::new();
        let loaded = store.set_data_dir(dir.path.clone());
        assert_eq!(loaded, 0);
        assert_eq!(store.podcast_count(), 0);
        assert_eq!(store.data_dir(), Some(dir.path.as_path()));
    }

    #[test]
    fn subscribe_writes_to_disk_when_bound() {
        let dir = TempDir::new();
        let mut store = PodcastStore::new();
        store.set_data_dir(dir.path.clone());
        store.subscribe(make_podcast("Disk Show"), vec![]);
        assert!(dir.path.join("podcasts.json").exists());
    }

    #[test]
    fn fresh_store_can_reload_after_subscribe() {
        let dir = TempDir::new();
        let podcast_id;
        let episodes;
        {
            let mut store = PodcastStore::new();
            store.set_data_dir(dir.path.clone());
            let podcast = make_podcast("Persistent Show");
            podcast_id = podcast.id;
            episodes = vec![make_episode(podcast_id, "Ep 1"), make_episode(podcast_id, "Ep 2")];
            store.subscribe(podcast, episodes.clone());
        }
        // New store, same dir — should rehydrate.
        let mut store2 = PodcastStore::new();
        let loaded = store2.set_data_dir(dir.path.clone());
        assert_eq!(loaded, 1);
        assert_eq!(store2.podcast_count(), 1);
        let restored = store2.podcast(podcast_id).expect("podcast restored");
        assert_eq!(restored.title, "Persistent Show");
        assert_eq!(store2.episodes_for(podcast_id).len(), 2);
        assert_eq!(store2.episodes_for(podcast_id), episodes.as_slice());
    }

    #[test]
    fn unsubscribe_writes_to_disk_when_bound() {
        let dir = TempDir::new();
        let mut store = PodcastStore::new();
        store.set_data_dir(dir.path.clone());
        let podcast = make_podcast("Doomed");
        let id = podcast.id;
        store.subscribe(podcast, vec![]);
        store.unsubscribe(id);

        // Reload — should be empty.
        let mut store2 = PodcastStore::new();
        let loaded = store2.set_data_dir(dir.path.clone());
        assert_eq!(loaded, 0);
        assert_eq!(store2.podcast_count(), 0);
    }

    #[test]
    fn update_refresh_metadata_writes_to_disk_when_bound() {
        let dir = TempDir::new();
        let mut store = PodcastStore::new();
        store.set_data_dir(dir.path.clone());
        let podcast = make_podcast("Etag Show");
        let id = podcast.id;
        store.subscribe(podcast, vec![]);
        store.update_refresh_metadata(id, Some("W/\"abc\"".into()), Some("Mon, 25 May".into()));

        let mut store2 = PodcastStore::new();
        store2.set_data_dir(dir.path.clone());
        let restored = store2.podcast(id).expect("podcast restored");
        assert_eq!(restored.etag.as_deref(), Some("W/\"abc\""));
        assert_eq!(restored.last_modified.as_deref(), Some("Mon, 25 May"));
        assert!(restored.last_refreshed_at.is_some());
    }

    #[test]
    fn set_data_dir_replaces_in_memory_state() {
        // If the store already has content in memory and a different data dir
        // is bound, the on-disk state from that dir wins (replaces in-mem).
        let dir = TempDir::new();
        // Pre-populate dir from store A.
        {
            let mut store_a = PodcastStore::new();
            store_a.set_data_dir(dir.path.clone());
            store_a.subscribe(make_podcast("From Disk"), vec![]);
        }
        // Store B starts with a different in-memory podcast, then binds.
        let mut store_b = PodcastStore::new();
        store_b.subscribe(make_podcast("Transient"), vec![]);
        assert_eq!(store_b.podcast_count(), 1);

        let loaded = store_b.set_data_dir(dir.path.clone());
        assert_eq!(loaded, 1);
        // The transient podcast was replaced by the one on disk.
        let titles: Vec<&str> = store_b.all_podcasts().iter().map(|(p, _)| p.title.as_str()).collect();
        assert_eq!(titles, vec!["From Disk"]);
    }

    #[test]
    fn store_without_data_dir_never_touches_disk() {
        // Sanity: in-memory only mode is the default and does not panic.
        let mut store = PodcastStore::new();
        store.subscribe(make_podcast("Memory Only"), vec![]);
        store.unsubscribe(PodcastId::generate()); // no-op
        assert_eq!(store.podcast_count(), 1);
        assert!(store.data_dir().is_none());
    }
}
