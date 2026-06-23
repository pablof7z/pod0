//! Podcast/subscription management methods for [`super::super::PodcastStore`].

use podcast_core::{Episode, Podcast, PodcastId};

use super::super::PodcastStore;

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
        self.notifications_disabled.remove(&podcast_id);
        if removed_p || removed_e || removed_f || removed_a {
            self.persist();
        }
    }

    /// Remove only the follow membership for a known podcast, keeping the
    /// podcast row and episodes as "known but unfollowed".
    ///
    /// This is the lightweight counterpart to `unsubscribe`: the user's
    /// episode cache survives so a re-subscribe via `mark_subscribed` is
    /// instant (no network fetch needed). Auto-download policy is cleared so
    /// a later re-subscribe starts from the default "off" state.
    ///
    /// Returns `false` when the podcast was not in the store (no-op).
    pub fn mark_unsubscribed(&mut self, podcast_id: PodcastId) -> bool {
        if !self.podcasts.contains_key(&podcast_id) {
            return false;
        }
        let removed = self.followed_podcasts.remove(&podcast_id);
        self.auto_download_enabled.remove(&podcast_id);
        self.auto_download_modes.remove(&podcast_id);
        self.auto_download_cellular_allowed.remove(&podcast_id);
        self.notifications_disabled.remove(&podcast_id);
        if removed {
            self.persist();
        }
        true
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

}
