//! Episode query and download-path tracking methods for [`super::super::PodcastStore`].

use std::collections::HashMap;

use podcast_core::{Chapter, EpisodeId, PodcastId};
use podcast_transcripts::TranscriptEntry;

use super::super::PodcastStore;

impl PodcastStore {
    // ── Episode lookup ────────────────────────────────────────────────────────

    /// Find episode playback info by the string form of its `EpisodeId` UUID.
    ///
    /// Returns `(canonical_episode_id, podcast_id_str, enclosure_url,
    /// position_secs)` or `None` when no episode with that id is found.
    ///
    /// `canonical_episode_id` is the store's own lowercase `Uuid::to_string`
    /// form — NOT necessarily the `id_str` the caller passed. The match is
    /// **case-insensitive** (same rationale as
    /// [`episode_enclosure_url`](Self::episode_enclosure_url)): the iOS shell
    /// dispatches `podcast.player` `load`/`play` with `UUID.uuidString`
    /// (UPPERCASE) while `Uuid::to_string` renders lowercase. A direct `==`
    /// never matched an iOS-sourced id, so `handle_play`/`handle_load` bailed
    /// before `actor.stage_load` — leaving the player actor's `episode_id`
    /// (and therefore `PodcastUpdate.now_playing` + the kernel-owned widget)
    /// empty even while audio played.
    ///
    /// Returning the canonical id lets the caller stage *that* on the actor, so
    /// every downstream consumer keyed on the actor's `episode_id` — the widget
    /// library lookup and the position writeback — matches the lowercase store
    /// form by exact `==` without each needing its own case-folding. A UUID is
    /// case-insensitive by spec.
    pub fn episode_playback_info(&self, id_str: &str) -> Option<(String, String, String, f64)> {
        for (podcast_id, episodes) in &self.episodes {
            if let Some(ep) = episodes
                .iter()
                .find(|e| e.id.0.to_string().eq_ignore_ascii_case(id_str))
            {
                return Some((
                    ep.id.0.to_string(),
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
            // Case-insensitive: iOS sends UPPERCASE `UUID.uuidString`; stored
            // ids render lowercase (see `episode_playback_info`).
            if let Some(ep) = episodes
                .iter()
                .find(|e| e.id.0.to_string().eq_ignore_ascii_case(id_str))
            {
                let pod = self.podcasts.get(podcast_id)?;
                return Some((ep.title.clone(), pod.title.clone(), ep.duration_secs));
            }
        }
        None
    }

    /// All context needed for transcript-refined, chapter-snapped AutoSnip —
    /// titles, chapters, duration, and timed transcript entries — fetched in a
    /// **single store-lock acquisition**.
    ///
    /// Returns `(episode_title, podcast_title, chapters, duration_secs,
    /// timed_entries)`.
    ///
    /// - `chapters` is `None` when the episode carries no chapter metadata; an
    ///   explicitly empty `Vec` is returned as `Some(vec![])` so the caller can
    ///   distinguish the two (both fall back to the ±30 s window, but they are
    ///   semantically different).
    /// - `timed_entries` is `None` when no structured transcript has been
    ///   ingested for this episode in the current session. When `None` the
    ///   caller skips the transcript-refine post-pass (falls back to
    ///   chapter-snap or ±30 s — S2 behavior).
    ///
    /// Neither field changes the wire shape.
    pub fn episode_auto_snip_context(
        &self,
        id_str: &str,
    ) -> Option<(String, String, Option<Vec<Chapter>>, Option<f64>, Option<Vec<TranscriptEntry>>)>
    {
        for (podcast_id, episodes) in &self.episodes {
            // Case-insensitive: iOS sends UPPERCASE `UUID.uuidString`; stored
            // ids render lowercase.
            if let Some(ep) = episodes
                .iter()
                .find(|e| e.id.0.to_string().eq_ignore_ascii_case(id_str))
            {
                let pod = self.podcasts.get(podcast_id)?;
                // Clone timed entries under the same lock (no 2nd acquisition).
                // Case-insensitive match — as robust as the episode lookup above.
                // `timed_transcripts` is keyed by whatever casing iOS reported in
                // `transcript_report`, so a plain `.get(id_str)` could silently
                // miss on a casing mismatch. We iterate + `eq_ignore_ascii_case`.
                // The INSERT key in `transcript_report.rs` is left untouched —
                // other readers (knowledge.rs) key in with the original casing.
                let timed = self
                    .timed_transcripts
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(id_str))
                    .map(|(_, v)| v.clone());
                return Some((
                    ep.title.clone(),
                    pod.title.clone(),
                    ep.chapters.clone(),
                    ep.duration_secs,
                    timed,
                ));
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

    /// Resolve metadata needed to publish a NIP-84 highlight for a clip.
    ///
    /// Returns `(enclosure_url, feed_url, item_guid)` with the GUID falling
    /// back to the stable episode UUID when a feedless/local episode has no
    /// publisher GUID.
    pub fn episode_highlight_metadata(
        &self,
        id_str: &str,
    ) -> Option<(String, Option<String>, String)> {
        for (podcast_id, episodes) in &self.episodes {
            if let Some(ep) = episodes
                .iter()
                .find(|e| e.id.0.to_string().eq_ignore_ascii_case(id_str))
            {
                let feed_url = self
                    .podcasts
                    .get(podcast_id)
                    .and_then(|pod| pod.feed_url.as_ref())
                    .map(|url| url.to_string());
                let item_guid = if ep.guid.is_empty() {
                    ep.id.0.to_string()
                } else {
                    ep.guid.clone()
                };
                return Some((ep.enclosure_url.to_string(), feed_url, item_guid));
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
        self.persist();
    }

    /// Look up the on-disk path of a downloaded enclosure, if any.
    pub fn local_path_for(&self, episode_id: &EpisodeId) -> Option<&str> {
        self.local_paths.get(episode_id).map(String::as_str)
    }

    /// Resolve the episode and cloned local path for a user-requested or
    /// policy-driven delete. Does not mutate state: callers must first remove
    /// the file, then call [`clear_local_path`](Self::clear_local_path) only
    /// when the filesystem operation succeeds or the path is already stale.
    pub fn download_delete_candidate(&self, id_str: &str) -> Option<(EpisodeId, String)> {
        let (episode_id, _url) = self.episode_enclosure_url(id_str)?;
        let path = self.local_path_for(&episode_id)?.to_owned();
        Some((episode_id, path))
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
            // Case-insensitive: iOS sends UPPERCASE `UUID.uuidString`; stored
            // ids render lowercase (see `episode_playback_info`).
            if episodes
                .iter()
                .any(|e| e.id.0.to_string().eq_ignore_ascii_case(episode_id_str))
            {
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
            // Case-insensitive: iOS sends UPPERCASE `UUID.uuidString`; stored
            // ids render lowercase (see `episode_playback_info`). A case
            // mismatch here would mis-report a downloaded episode as needing a
            // re-download on every play.
            if let Some(ep) = episodes
                .iter()
                .find(|e| e.id.0.to_string().eq_ignore_ascii_case(id_str))
            {
                return self.local_paths.contains_key(&ep.id);
            }
        }
        false
    }

    /// Remove the local-path mapping for an episode and return the path that
    /// was previously stored so the caller can remove the file.
    pub fn clear_local_path(&mut self, episode_id: &EpisodeId) -> Option<String> {
        self.file_sizes.remove(episode_id);
        let removed = self.local_paths.remove(episode_id);
        if removed.is_some() {
            self.persist();
        }
        removed
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
                let ep_guid: &str = if !ep.guid.is_empty() {
                    &ep.guid
                } else {
                    &id_str
                };
                if ep_guid == guid {
                    return Some(id_str);
                }
            }
        }
        None
    }
}
