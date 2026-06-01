//! `PodcastStore` extensions for NIP-F4 owned-podcast publishing
//! (features #27/#28).
//!
//! Lives in a sibling file to keep [`super::mod`] under the 500-LOC hard
//! limit (AGENTS.md). The methods are plain `impl PodcastStore` —
//! Rust's coherence rules allow multiple `impl` blocks for the same
//! type within a crate.

use podcast_core::{Episode, NostrVisibility, Podcast, PodcastId, PodcastKind};

use super::PodcastStore;

impl PodcastStore {
    /// Insert a synthetic (feed-less) podcast row from full agent-supplied
    /// metadata. `podcast_id_str` is the Swift-minted UUID so both stores
    /// agree on identity; an unparseable UUID is a silent no-op (the publish
    /// module surfaces the error). Idempotent on id — re-inserting replaces
    /// the row (mirrors [`Self::subscribe`]'s upsert semantics) so a retried
    /// create doesn't duplicate.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_synthetic_podcast(
        &mut self,
        podcast_id_str: &str,
        title: String,
        description: String,
        author: String,
        artwork_url: Option<String>,
        language: Option<String>,
        categories: Vec<String>,
        visibility: NostrVisibility,
    ) -> bool {
        let Ok(uuid) = uuid::Uuid::parse_str(podcast_id_str) else {
            return false;
        };
        let id = PodcastId(uuid);
        let mut podcast = Podcast::new(title);
        podcast.id = id;
        podcast.kind = PodcastKind::Synthetic;
        podcast.feed_url = None;
        podcast.description = description;
        podcast.author = author;
        podcast.image_url = artwork_url.and_then(|u| url::Url::parse(&u).ok());
        podcast.language = language;
        podcast.categories = categories;
        podcast.nostr_visibility = visibility;
        self.podcasts.insert(id, podcast);
        self.episodes.entry(id).or_default();
        self.persist();
        true
    }

    /// Apply a partial metadata update to an owned podcast row. `None`
    /// fields keep the current value. An `artwork_url` that fails to parse
    /// is ignored (the prior image is kept) rather than blanking the field.
    /// Returns `true` when the row existed and was updated.
    ///
    /// `author` and `visibility` are carried so the kernel store stays the
    /// single source of truth — without them a Swift-side author edit or
    /// visibility flip would be clobbered by the next snapshot push (the
    /// projection rebuilds `state.podcasts` wholesale from the kernel row).
    #[allow(clippy::too_many_arguments)]
    pub fn update_owned_metadata(
        &mut self,
        podcast_id_str: &str,
        title: Option<String>,
        description: Option<String>,
        author: Option<String>,
        artwork_url: Option<String>,
        visibility: Option<NostrVisibility>,
    ) -> bool {
        let Some(id) = self.id_for_str(podcast_id_str) else {
            return false;
        };
        let Some(p) = self.podcasts.get_mut(&id) else {
            return false;
        };
        if let Some(t) = title {
            p.title = t;
        }
        if let Some(d) = description {
            p.description = d;
        }
        if let Some(a) = author {
            p.author = a;
        }
        if let Some(a) = artwork_url {
            if let Ok(url) = url::Url::parse(&a) {
                p.image_url = Some(url);
            }
        }
        if let Some(v) = visibility {
            p.nostr_visibility = v;
        }
        self.persist();
        true
    }

    /// Remove a podcast row and all its episodes. Used by the owned-podcast
    /// delete lifecycle. Mirror of [`Self::unsubscribe`] but keyed by the
    /// UUID string the publish module carries. Silent no-op when not found.
    pub fn remove_podcast_and_episodes(&mut self, podcast_id_str: &str) {
        let Some(id) = self.id_for_str(podcast_id_str) else {
            return;
        };
        self.podcasts.remove(&id);
        self.episodes.remove(&id);
        self.auto_download_enabled.remove(&id);
        self.auto_download_cellular_allowed.remove(&id);
        self.persist();
    }

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
#[path = "owned_ext_tests.rs"]
mod tests;
