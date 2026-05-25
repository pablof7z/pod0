//! `PodcastStore` extensions for NIP-F4 owned-podcast publishing
//! (features #27/#28).
//!
//! Lives in a sibling file to keep [`super::mod`] under the 500-LOC hard
//! limit (AGENTS.md). The methods are plain `impl PodcastStore` —
//! Rust's coherence rules allow multiple `impl` blocks for the same
//! type within a crate.

use podcast_core::{Episode, Podcast};

use super::PodcastStore;

impl PodcastStore {
    /// Stamp `owner_pubkey_hex` onto a podcast row, flushing to disk
    /// when a data dir is bound. Silent no-op when the podcast isn't
    /// found — the caller (NIP-F4 publish module) has already
    /// generated a key and treats a missing podcast as "out of band".
    pub fn set_owner_pubkey_hex(&mut self, podcast_id_str: &str, pubkey_hex: String) {
        if let Some(id) = self.id_for_str(podcast_id_str) {
            if let Some(p) = self.podcasts.get_mut(&id) {
                p.owner_pubkey_hex = Some(pubkey_hex);
                self.persist();
            }
        }
    }

    /// Clear the owner pubkey on a podcast row. Mirror of
    /// [`Self::set_owner_pubkey_hex`] for the `remove_owned_podcast` op.
    pub fn clear_owner_pubkey_hex(&mut self, podcast_id_str: &str) {
        if let Some(id) = self.id_for_str(podcast_id_str) {
            if let Some(p) = self.podcasts.get_mut(&id) {
                p.owner_pubkey_hex = None;
                self.persist();
            }
        }
    }

    /// Resolve an episode UUID string to a cloned `(Podcast, Episode)`
    /// pair. Used by the NIP-F4 publish module to build a `kind:54`
    /// event without holding the store lock across the tag construction.
    pub fn episode_with_podcast_clone(
        &self,
        episode_id_str: &str,
    ) -> Option<(Podcast, Episode)> {
        for (podcast_id, episodes) in &self.episodes {
            if let Some(ep) = episodes.iter().find(|e| e.id.0.to_string() == episode_id_str) {
                if let Some(p) = self.podcasts.get(podcast_id) {
                    return Some((p.clone(), ep.clone()));
                }
            }
        }
        None
    }

    /// Helper — resolve the string form of a podcast UUID back to the
    /// typed key. Returns `None` when nothing matches.
    fn id_for_str(&self, podcast_id_str: &str) -> Option<podcast_core::PodcastId> {
        self.podcasts
            .values()
            .find(|p| p.id.0.to_string() == podcast_id_str)
            .map(|p| p.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use podcast_core::{Episode, Podcast, PodcastId};
    use url::Url;
    use uuid::Uuid;

    fn fixture_episode(podcast_id: PodcastId, title: &str) -> Episode {
        Episode::new(
            podcast_id,
            format!("guid-{}", Uuid::new_v4()),
            title,
            Url::parse("https://example.com/audio.mp3").unwrap(),
            chrono::Utc::now(),
        )
    }

    #[test]
    fn set_and_clear_owner_pubkey_hex_round_trip() {
        let mut store = PodcastStore::new();
        let podcast = Podcast::new("Owned");
        let id_str = podcast.id.0.to_string();
        store.subscribe(podcast, vec![]);

        store.set_owner_pubkey_hex(&id_str, "abc123".into());
        assert_eq!(
            store.podcast_by_id_str(&id_str).and_then(|p| p.owner_pubkey_hex.clone()),
            Some("abc123".into())
        );

        store.clear_owner_pubkey_hex(&id_str);
        assert_eq!(
            store.podcast_by_id_str(&id_str).and_then(|p| p.owner_pubkey_hex.clone()),
            None
        );
    }

    #[test]
    fn set_owner_pubkey_hex_silently_ignores_unknown_podcast() {
        let mut store = PodcastStore::new();
        // No panic, no state change.
        store.set_owner_pubkey_hex("never-subscribed", "abc".into());
        store.clear_owner_pubkey_hex("never-subscribed");
        assert_eq!(store.podcast_count(), 0);
    }

    #[test]
    fn episode_with_podcast_clone_returns_pair_for_known_episode() {
        let mut store = PodcastStore::new();
        let podcast = Podcast::new("Source");
        let pid = podcast.id;
        let ep = fixture_episode(pid, "Pilot");
        let eid_str = ep.id.0.to_string();
        store.subscribe(podcast, vec![ep]);

        let (p_out, e_out) = store
            .episode_with_podcast_clone(&eid_str)
            .expect("found");
        assert_eq!(p_out.id, pid);
        assert_eq!(e_out.title, "Pilot");
    }

    #[test]
    fn episode_with_podcast_clone_returns_none_for_unknown_episode() {
        let store = PodcastStore::new();
        assert!(store.episode_with_podcast_clone("no-such-episode").is_none());
    }
}
