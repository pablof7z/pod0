//! In-memory podcast library store.
//!
//! Holds the set of subscribed podcasts and their episodes. Keyed by `PodcastId`
//! so lookups are O(1); the store is wrapped in `Arc<Mutex<PodcastStore>>` and
//! shared between the `PodcastHandle` (snapshot reader) and the
//! `PodcastHostOpHandler` (writer). All writes happen on the actor thread;
//! reads happen on the iOS main thread via `nmp_app_podcast_snapshot`.

use std::collections::HashMap;

use podcast_core::{Episode, Podcast, PodcastId};

/// In-memory store for subscribed podcasts and their episode lists.
pub struct PodcastStore {
    podcasts: HashMap<PodcastId, Podcast>,
    episodes: HashMap<PodcastId, Vec<Episode>>,
}

impl PodcastStore {
    pub fn new() -> Self {
        Self {
            podcasts: HashMap::new(),
            episodes: HashMap::new(),
        }
    }

    /// Add or replace a podcast and its episode list.
    ///
    /// Idempotent: re-subscribing to the same feed URL replaces the existing
    /// record so a "refresh" can use the same code path as "subscribe".
    pub fn subscribe(&mut self, podcast: Podcast, episodes: Vec<Episode>) {
        let id = podcast.id;
        self.podcasts.insert(id, podcast);
        self.episodes.insert(id, episodes);
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

    /// Remove a podcast and all its episodes. Silent no-op when not found.
    pub fn unsubscribe(&mut self, podcast_id: PodcastId) {
        self.podcasts.remove(&podcast_id);
        self.episodes.remove(&podcast_id);
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
    /// feed refresh without replacing the entire podcast record.
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
}

impl Default for PodcastStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use podcast_core::Podcast;

    fn make_podcast(title: &str) -> Podcast {
        Podcast::new(title)
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
}
