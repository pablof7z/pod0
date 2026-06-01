//! `PodcastStore` extensions for NIP-F4 owned-podcast publishing
//! (features #27/#28).
//!
//! Lives in a sibling file to keep [`super::mod`] under the 500-LOC hard
//! limit (AGENTS.md). The methods are plain `impl PodcastStore` —
//! Rust's coherence rules allow multiple `impl` blocks for the same
//! type within a crate.

use podcast_core::types::chapter::{Chapter, ChapterSource};
use podcast_core::types::download::DownloadState;
use podcast_core::types::transcript::{TranscriptSource, TranscriptState};
use podcast_core::{Episode, EpisodeId, NostrVisibility, Podcast, PodcastId, PodcastKind};

use super::PodcastStore;

/// One agent-supplied chapter for a synthetic episode. Carries the parity
/// fields the Swift TTS composer used to build directly on `Episode.Chapter`:
/// `image_url` (mid-play artwork swap for snippet chapters) and
/// `source_episode_id` (the source-episode chip). All chapters are marked
/// AI-generated.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SyntheticChapter {
    pub start_secs: f64,
    pub title: String,
    pub image_url: Option<String>,
    pub source_episode_id: Option<String>,
}

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

    /// Insert (or replace) an agent-generated episode under a synthetic
    /// podcast. The kernel becomes the source of truth so the episode survives
    /// the projection full-replace tick (Swift no longer holds the only copy).
    ///
    /// `podcast_id_str` / `episode_id_str` are the Swift-minted UUIDs so both
    /// stores agree on identity; an unparseable id, or a podcast id with no
    /// existing episode bucket, is a no-op returning `false` (the publish
    /// module surfaces the error). Idempotent on `episode_id` — re-registering
    /// replaces the prior row in place so a retried create doesn't duplicate.
    ///
    /// The episode is wired for immediate local playback: `enclosure_url` is a
    /// `file://` URL over `audio_path`, `download_state` is `Downloaded`, the
    /// local-path side-map points at `audio_path` (this is what projects as
    /// `EpisodeSummary.download_path`), `played = false`, `position_secs = 0`.
    /// Chapters are stored verbatim (all `is_ai_generated`); when `transcript`
    /// is present it is cached as the flat episode transcript and the state is
    /// flipped to `Ready { source: Other }`. `ad_segments` is set to empty so
    /// the already-structured episode is not re-processed.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_synthetic_episode(
        &mut self,
        podcast_id_str: &str,
        episode_id_str: &str,
        title: String,
        audio_path: &str,
        duration_secs: Option<f64>,
        chapters: Vec<SyntheticChapter>,
        transcript: Option<String>,
    ) -> bool {
        let Some(podcast_id) = self.id_for_str(podcast_id_str) else {
            return false;
        };
        let Ok(episode_uuid) = uuid::Uuid::parse_str(episode_id_str) else {
            return false;
        };
        let Some(file_url) = path_to_file_url(audio_path) else {
            return false;
        };
        let episode_id = EpisodeId(episode_uuid);

        let byte_count = std::fs::metadata(audio_path)
            .map(|m| m.len() as i64)
            .unwrap_or(0);

        // Episode::new derives a feed-and-guid id; override with the
        // Swift-minted UUID so it is the stable key everything else (positions,
        // local paths, publish lookups) is keyed off.
        let mut episode = Episode::new(
            podcast_id,
            "agent-generated://podcast",
            episode_id_str,
            title,
            file_url.clone(),
            chrono::Utc::now(),
        );
        episode.id = episode_id;
        episode.duration_secs = duration_secs;
        episode.enclosure_mime_type = Some("audio/mp4".to_string());
        episode.image_url = chapters
            .iter()
            .find_map(|c| c.image_url.as_deref())
            .and_then(|u| url::Url::parse(u).ok());
        episode.chapters = Some(
            chapters
                .iter()
                .map(|c| {
                    let mut ch = Chapter::new(c.title.clone(), c.start_secs);
                    ch.is_ai_generated = true;
                    ch.source = ChapterSource::Llm;
                    ch.image_url = c.image_url.as_deref().and_then(|u| url::Url::parse(u).ok());
                    ch.source_episode_id = c.source_episode_id.clone();
                    ch
                })
                .collect(),
        );
        episode.download_state = DownloadState::Downloaded {
            local_file_url: file_url,
            byte_count,
        };
        episode.position_secs = 0.0;
        episode.played = false;
        if transcript.is_some() {
            episode.transcript_state = TranscriptState::Ready {
                source: TranscriptSource::Other,
            };
        }

        // Replace any prior copy of this episode id, then append.
        let bucket = self.episodes.entry(podcast_id).or_default();
        bucket.retain(|e| e.id != episode_id);
        bucket.push(episode);

        // Side-maps: local download path (drives `download_path`), flat
        // transcript text, and an explicit empty ad-segment list.
        self.set_local_path(episode_id, audio_path.to_string(), byte_count);
        if let Some(text) = transcript {
            self.set_transcript(episode_id_str.to_string(), text);
        }
        self.set_ad_segments_for(episode_id_str.to_string(), Vec::new());

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

/// Build a `file://` [`url::Url`] from a local filesystem path. Accepts both an
/// already-formed `file://` string (round-trips it) and a bare absolute path
/// (Swift passes `URL.path`, not `URL.absoluteString`). Returns `None` for an
/// empty path or a value `Url::from_file_path` rejects (e.g. a relative path).
fn path_to_file_url(audio_path: &str) -> Option<url::Url> {
    if audio_path.is_empty() {
        return None;
    }
    if audio_path.starts_with("file://") {
        return url::Url::parse(audio_path).ok();
    }
    url::Url::from_file_path(audio_path).ok()
}

#[cfg(test)]
#[path = "owned_ext_tests.rs"]
mod tests;
