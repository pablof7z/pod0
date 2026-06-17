//! Podcast library management and episode lookup for [`super::PodcastStore`].
//!
//! Extracted from `store/mod.rs` to keep that file within the 300-line soft
//! limit. Covers known-podcast lifecycle, read-only podcast/episode queries,
//! download-path tracking, and episode metadata resolution.

use std::collections::HashMap;

use podcast_core::{Chapter, Episode, EpisodeId, Podcast, PodcastId};

use super::PodcastStore;

impl PodcastStore {
    /// Add or replace a known podcast and its episode list without changing
    /// whether the user follows it.
    ///
    /// Idempotent on podcast id: refreshes, external-play metadata hydration,
    /// and unfollowed-feed ensure all use this path so the Rust store stays the
    /// source of truth without manufacturing a subscription row.
    pub fn upsert_known_podcast(&mut self, podcast: Podcast, episodes: Vec<Episode>) {
        let id = podcast.id;
        self.podcasts.insert(id, podcast);
        self.episodes.insert(id, episodes);
        self.persist();
    }

    /// Add or replace a podcast and mark it followed, flushing to disk if a
    /// data dir is registered.
    ///
    /// Idempotent: re-subscribing to the same feed URL replaces the existing
    /// record and keeps exactly one follow membership entry.
    pub fn subscribe(&mut self, podcast: Podcast, episodes: Vec<Episode>) {
        let id = podcast.id;
        self.podcasts.insert(id, podcast);
        self.episodes.insert(id, episodes);
        self.followed_podcasts.insert(id);
        self.persist();
    }

    /// Mark an already-known podcast as followed. Returns `false` when the
    /// podcast row does not exist.
    pub fn mark_subscribed(&mut self, podcast_id: PodcastId) -> bool {
        if !self.podcasts.contains_key(&podcast_id) {
            return false;
        }
        let changed = self.followed_podcasts.insert(podcast_id);
        if changed {
            self.persist();
        }
        true
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
        let removed_f = self.followed_podcasts.remove(&podcast_id);
        let removed_a = self.auto_download_enabled.remove(&podcast_id);
        self.auto_download_modes.remove(&podcast_id);
        self.auto_download_cellular_allowed.remove(&podcast_id);
        if removed_p || removed_e || removed_f || removed_a {
            self.persist();
        }
    }

    /// Iterate over all known podcasts and their episode slices.
    pub fn all_podcasts(&self) -> Vec<(&Podcast, &[Episode])> {
        let mut result = Vec::with_capacity(self.podcasts.len());
        for (id, podcast) in &self.podcasts {
            let eps = self.episodes.get(id).map(Vec::as_slice).unwrap_or(&[]);
            result.push((podcast, eps));
        }
        result
    }

    /// Iterate over followed podcasts and their episode slices.
    pub fn subscribed_podcasts(&self) -> Vec<(&Podcast, &[Episode])> {
        let mut result = Vec::with_capacity(self.followed_podcasts.len());
        for id in &self.followed_podcasts {
            if let Some(podcast) = self.podcasts.get(id) {
                let eps = self.episodes.get(id).map(Vec::as_slice).unwrap_or(&[]);
                result.push((podcast, eps));
            }
        }
        result
    }

    pub fn podcast_count(&self) -> usize {
        self.podcasts.len()
    }

    pub fn episodes_for(&self, podcast_id: PodcastId) -> &[Episode] {
        self.episodes
            .get(&podcast_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn podcast(&self, podcast_id: PodcastId) -> Option<&Podcast> {
        self.podcasts.get(&podcast_id)
    }

    pub fn is_subscribed(&self, podcast_id: PodcastId) -> bool {
        self.followed_podcasts.contains(&podcast_id)
    }

    /// Look up a podcast by the string form of its UUID.
    pub fn podcast_by_id_str(&self, id_str: &str) -> Option<&Podcast> {
        self.podcasts
            .values()
            .find(|p| p.id.0.to_string() == id_str)
    }

    /// Return the known podcast row for a feed URL.
    pub fn podcast_by_feed_url(&self, url: &url::Url) -> Option<&Podcast> {
        self.podcasts
            .values()
            .find(|p| p.feed_url.as_ref() == Some(url))
    }

    /// Return `true` when a podcast with the given RSS feed URL is known.
    pub fn has_feed_url(&self, url: &url::Url) -> bool {
        self.podcast_by_feed_url(url).is_some()
    }

    /// Return `true` when a feed URL is already followed by the user.
    pub fn has_subscribed_feed_url(&self, url: &url::Url) -> bool {
        self.podcast_by_feed_url(url)
            .map(|p| self.is_subscribed(p.id))
            .unwrap_or(false)
    }

    /// Return `(id, feed_url, etag, last_modified)` for every followed podcast
    /// that has an RSS feed URL. Used by `refresh_all`.
    pub fn all_feed_infos(&self) -> Vec<(PodcastId, url::Url, Option<String>, Option<String>)> {
        self.podcasts
            .values()
            .filter(|p| self.is_subscribed(p.id))
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

    /// All context needed for chapter-snapped AutoSnip — titles, chapters, and
    /// duration — fetched in a **single store-lock acquisition**.
    ///
    /// Returns `(episode_title, podcast_title, chapters, duration_secs)`.
    /// `chapters` is `None` when the episode carries no chapter metadata; an
    /// explicitly empty `Vec` is returned as `Some(vec![])` so the caller can
    /// distinguish the two (both fall back to the ±30 s window, but they are
    /// semantically different). Neither case changes the wire shape.
    pub fn episode_auto_snip_context(
        &self,
        id_str: &str,
    ) -> Option<(String, String, Option<Vec<Chapter>>, Option<f64>)> {
        for (podcast_id, episodes) in &self.episodes {
            // Case-insensitive: iOS sends UPPERCASE `UUID.uuidString`; stored
            // ids render lowercase.
            if let Some(ep) = episodes
                .iter()
                .find(|e| e.id.0.to_string().eq_ignore_ascii_case(id_str))
            {
                let pod = self.podcasts.get(podcast_id)?;
                return Some((
                    ep.title.clone(),
                    pod.title.clone(),
                    ep.chapters.clone(),
                    ep.duration_secs,
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

    /// Find the `PodcastId` of the feedless show keyed by `owner_pubkey_hex`.
    ///
    /// Used by [`NostrEpisodesObserver`] to route inbound `kind:54` events to
    /// the right podcast row without holding the store lock across I/O.
    pub fn podcast_id_for_pubkey(&self, pubkey_hex: &str) -> Option<PodcastId> {
        self.podcasts
            .values()
            .find(|p| p.owner_pubkey_hex.as_deref() == Some(pubkey_hex))
            .map(|p| p.id)
    }

    /// Ensure a feedless (no RSS `feed_url`) followed show row exists for
    /// `owner_pubkey_hex`. Creates the row if absent; idempotent if present.
    ///
    /// Used by `handle_subscribe_nostr` to make the show appear in the library
    /// immediately — before any `kind:54` episode events arrive.
    pub fn subscribe_feedless_show(&mut self, owner_pubkey_hex: &str, show_title: &str) {
        if self.podcast_id_for_pubkey(owner_pubkey_hex).is_some() {
            return; // already exists — no-op.
        }
        let id = PodcastId::new(uuid::Uuid::new_v5(
            &uuid::Uuid::NAMESPACE_URL,
            format!("nostr:show:{owner_pubkey_hex}").as_bytes(),
        ));
        let mut podcast = Podcast::new(show_title.to_string());
        podcast.id = id;
        podcast.owner_pubkey_hex = Some(owner_pubkey_hex.to_string());
        podcast.nostr_coordinate =
            Some(format!("{}:{owner_pubkey_hex}", podcast_discovery::KIND_NIP_F4_SHOW));
        self.podcasts.insert(id, podcast);
        self.episodes.entry(id).or_default();
        self.followed_podcasts.insert(id);
        self.persist();
    }

    /// Upsert a feedless (no RSS `feed_url`) show row keyed by `owner_pubkey_hex`,
    /// then upsert the given episode into its episode list.
    ///
    /// If no podcast row with that pubkey exists, a minimal one is created
    /// (followed, no feed) so the existing snapshot projection / playback /
    /// download pipeline picks it up with zero changes — the show appears in the
    /// library as a subscribed feedless row. Re-entrant on pubkey: a second
    /// episode for the same show updates the existing row.
    ///
    /// Episode dedup is by `episode.id` — a re-arrival updates the row in place
    /// rather than appending a duplicate.
    pub fn upsert_feedless_episode(
        &mut self,
        owner_pubkey_hex: &str,
        show_title: &str,
        episode: Episode,
    ) {
        // Find or create the podcast row for this pubkey.
        let podcast_id = if let Some(id) = self.podcast_id_for_pubkey(owner_pubkey_hex) {
            id
        } else {
            let id = PodcastId::new(uuid::Uuid::new_v5(
                &uuid::Uuid::NAMESPACE_URL,
                format!("nostr:show:{owner_pubkey_hex}").as_bytes(),
            ));
            let mut podcast = Podcast::new(show_title.to_string());
            podcast.id = id;
            podcast.owner_pubkey_hex = Some(owner_pubkey_hex.to_string());
            podcast.nostr_coordinate =
                Some(format!("{}:{owner_pubkey_hex}", podcast_discovery::KIND_NIP_F4_SHOW));
            self.podcasts.insert(id, podcast);
            self.episodes.entry(id).or_default();
            self.followed_podcasts.insert(id);
            id
        };

        // Upsert the episode: update in place if the id already exists.
        let eps = self.episodes.entry(podcast_id).or_default();
        match eps.iter_mut().find(|e| e.id == episode.id) {
            Some(existing) => *existing = episode,
            None => eps.push(episode),
        }
        self.persist();
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
