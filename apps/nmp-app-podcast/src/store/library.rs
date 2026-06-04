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
        self.auto_download_cellular_allowed.remove(&podcast_id);
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
    ///
    /// Match is **case-insensitive**: a UUID is case-insensitive by spec, and
    /// the iOS shell sends `UUID.uuidString` (UPPERCASE) while `Uuid::to_string`
    /// renders lowercase. A direct `==` therefore never matched an iOS-sourced
    /// id — silently dropping the `Completed` download report's `set_local_path`
    /// so a finished download never flipped to `.downloaded`.
    pub fn episode_enclosure_url(&self, id_str: &str) -> Option<(EpisodeId, String)> {
        for episodes in self.episodes.values() {
            if let Some(ep) = episodes
                .iter()
                .find(|e| e.id.0.to_string().eq_ignore_ascii_case(id_str))
            {
                return Some((ep.id, ep.enclosure_url.to_string()));
            }
        }
        None
    }

    // ── Download local-path tracking ──────────────────────────────────────────

    /// Record the on-disk path and byte size of a successfully downloaded
    /// enclosure. `byte_count` is the file size measured at the download
    /// boundary (`std::fs::metadata`), kept lifecycle-locked to the path so
    /// the snapshot projection can surface `EpisodeSummary::file_size_bytes`
    /// without statting the file on every (main-thread) projection tick.
    /// Pass `0` when the size is unknown.
    pub fn set_local_path(&mut self, episode_id: EpisodeId, path: String, byte_count: i64) {
        self.local_paths.insert(episode_id, path);
        self.file_sizes.insert(episode_id, byte_count);
    }

    /// Look up the on-disk path of a downloaded enclosure, if any.
    pub fn local_path_for(&self, episode_id: &EpisodeId) -> Option<&str> {
        self.local_paths.get(episode_id).map(String::as_str)
    }

    /// Look up the recorded byte size of a downloaded enclosure. Returns
    /// `None` when the episode has no tracked download (mirrors
    /// [`local_path_for`](Self::local_path_for)); the projection treats that
    /// as `0`.
    pub fn file_size_for(&self, episode_id: &EpisodeId) -> Option<i64> {
        self.file_sizes.get(episode_id).copied()
    }

    /// Return the `PodcastId` for the podcast that owns `episode_id_str`, or
    /// `None` when the episode is unknown. Used for validation before dispatch.
    pub fn podcast_id_for_episode(&self, episode_id_str: &str) -> Option<podcast_core::PodcastId> {
        for (podcast_id, episodes) in &self.episodes {
            if episodes.iter().any(|e| e.id.0.to_string() == episode_id_str) {
                return Some(*podcast_id);
            }
        }
        None
    }

    /// Returns `true` when the episode's audio file has a locally tracked path.
    /// Used by `handle_play` to enqueue a background download for episodes that
    /// are streamed rather than played from local storage.
    pub fn episode_is_downloaded(&self, id_str: &str) -> bool {
        for episodes in self.episodes.values() {
            if let Some(ep) = episodes.iter().find(|e| e.id.0.to_string() == id_str) {
                return self.local_paths.contains_key(&ep.id);
            }
        }
        false
    }

    /// Remove the local-path mapping for an episode and return the path that
    /// was previously stored so the caller can remove the file.
    pub fn clear_local_path(&mut self, episode_id: &EpisodeId) -> Option<String> {
        self.file_sizes.remove(episode_id);
        self.local_paths.remove(episode_id)
    }

    /// Read-only access to the on-disk path side-map. Used by the
    /// auto-download policy helper so a "freshly discovered" episode
    /// already known to be on disk is not re-queued.
    pub fn local_paths(&self) -> &HashMap<EpisodeId, String> {
        &self.local_paths
    }

    /// Reverse-lookup: find the episode_id whose NIP-73 anchor matches
    /// `anchor` (`"podcast:item:guid:<guid>"`). Used by [`CommentsObserver`]
    /// to route inbound kind:1111 events to the right cache slot.
    pub fn episode_id_for_anchor(&self, anchor: &str) -> Option<String> {
        let guid = anchor.strip_prefix("podcast:item:guid:")?;
        for (_podcast, episodes) in self.all_podcasts() {
            for ep in episodes {
                let id_str = ep.id.0.to_string();
                let ep_guid: &str = if !ep.guid.is_empty() { &ep.guid } else { &id_str };
                if ep_guid == guid {
                    return Some(id_str);
                }
            }
        }
        None
    }
}
