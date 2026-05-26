//! Podcast library management and episode lookup for [`super::PodcastStore`].
//!
//! Extracted from `store/mod.rs` to keep that file within the 300-line soft
//! limit. Covers subscription lifecycle, read-only podcast/episode queries,
//! download-path tracking, and episode metadata resolution.

use std::collections::HashMap;

use podcast_core::{Episode, EpisodeId, Podcast, PodcastId};

use super::PodcastStore;

impl PodcastStore {
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

    // ── Episode lookup ────────────────────────────────────────────────────────

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

    // ── Download local-path tracking ──────────────────────────────────────────

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
}
