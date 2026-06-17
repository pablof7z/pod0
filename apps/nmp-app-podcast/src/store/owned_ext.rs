//! `PodcastStore` extensions for first-class podcast/episode creation and the
//! NIP-F4 owned-podcast publishing lifecycle (features #27/#28).
//!
//! Lives in a sibling file to keep [`super::mod`] under the 500-LOC hard
//! limit (AGENTS.md). The methods are plain `impl PodcastStore` —
//! Rust's coherence rules allow multiple `impl` blocks for the same
//! type within a crate.

use podcast_core::types::chapter::{Chapter, ChapterSource};
use podcast_core::types::download::DownloadState;
use podcast_core::types::transcript::{TranscriptSource, TranscriptState};
use podcast_core::{Episode, EpisodeId, NostrVisibility, Podcast, PodcastId};

use super::PodcastStore;

/// One caller-supplied chapter for an episode added via
/// [`PodcastStore::add_episode`]. Carries the parity fields the Swift TTS
/// composer used to build directly on `Episode.Chapter`: `image_url` (mid-play
/// artwork swap for snippet chapters) and `source_episode_id` (the
/// source-episode chip). All chapters are marked AI-generated.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EpisodeChapter {
    pub start_secs: f64,
    pub title: String,
    pub image_url: Option<String>,
    pub source_episode_id: Option<String>,
}

impl PodcastStore {
    /// Insert (or replace) a podcast row from full caller-supplied metadata.
    /// `podcast_id_str` is the Swift-minted UUID so both stores agree on
    /// identity; an unparseable UUID is a silent no-op (the caller surfaces the
    /// error). Idempotent on id — re-inserting replaces the row (mirrors
    /// [`Self::subscribe`]'s upsert semantics) so a retried create doesn't
    /// duplicate, and an enriched re-create updates the row in place.
    ///
    /// `feed_url` distinguishes a feed-backed show (e.g. an external-play
    /// placeholder) from a feed-less agent-owned/TTS show (`None`). An
    /// unparseable `feed_url` is dropped rather than failing the insert.
    /// `title_is_placeholder` marks the title as a provisional feed-host
    /// fallback awaiting metadata hydration.
    #[allow(clippy::too_many_arguments)]
    pub fn create_podcast(
        &mut self,
        podcast_id_str: &str,
        title: String,
        description: String,
        author: String,
        feed_url: Option<String>,
        artwork_url: Option<String>,
        language: Option<String>,
        categories: Vec<String>,
        visibility: NostrVisibility,
        title_is_placeholder: bool,
    ) -> bool {
        let Ok(uuid) = uuid::Uuid::parse_str(podcast_id_str) else {
            return false;
        };
        let id = PodcastId(uuid);
        let mut podcast = Podcast::new(title);
        podcast.id = id;
        podcast.feed_url = feed_url.and_then(|u| url::Url::parse(&u).ok());
        podcast.description = description;
        podcast.author = author;
        podcast.image_url = artwork_url.and_then(|u| url::Url::parse(&u).ok());
        podcast.language = language;
        podcast.categories = categories;
        podcast.nostr_visibility = visibility;
        podcast.title_is_placeholder = title_is_placeholder;
        self.podcasts.insert(id, podcast);
        self.episodes.entry(id).or_default();
        self.persist();
        true
    }

    /// Insert (or replace) an episode under a podcast. The kernel is the source
    /// of truth so the episode survives the projection full-replace tick (Swift
    /// no longer holds the only copy).
    ///
    /// `podcast_id_str` / `episode_id_str` are the Swift-minted UUIDs so both
    /// stores agree on identity; an unparseable id, an unusable `enclosure_url`,
    /// or a podcast id with no existing episode bucket, is a no-op returning
    /// `false` (the caller surfaces the error). Idempotent on `episode_id` —
    /// re-adding replaces the prior row in place so a retried create doesn't
    /// duplicate.
    ///
    /// `enclosure_url` drives the download wiring, branching on scheme:
    ///   * `file://` URL or a bare absolute path → the audio is already on disk
    ///     (TTS / agent-generated output): `download_state = Downloaded`, the
    ///     local-path side-map points at the file (this is what projects as
    ///     `EpisodeSummary.download_path`), and `enclosure_mime_type` is
    ///     `audio/mp4`.
    ///   * `http(s)://` URL → a remote enclosure (RSS / external audio):
    ///     `download_state = NotDownloaded` and no local path is set — the
    ///     normal download capability handles fetching later.
    ///
    /// `played = false`, `position_secs = 0`. Chapters are stored verbatim (all
    /// `is_ai_generated`); `image_url` overrides the per-episode artwork (RSS
    /// episodes have no chapters to derive it from). When `transcript` is
    /// present it is cached as the flat episode transcript and the state flips
    /// to `Ready { source: Other }`. `ad_segments` is set to empty so an
    /// already-structured episode is not re-processed.
    #[allow(clippy::too_many_arguments)]
    pub fn add_episode(
        &mut self,
        podcast_id_str: &str,
        episode_id_str: &str,
        title: String,
        enclosure_url: &str,
        description: String,
        duration_secs: Option<f64>,
        image_url: Option<String>,
        chapters: Vec<EpisodeChapter>,
        transcript: Option<String>,
    ) -> bool {
        let Some(podcast_id) = self.id_for_str(podcast_id_str) else {
            return false;
        };
        let Ok(episode_uuid) = uuid::Uuid::parse_str(episode_id_str) else {
            return false;
        };
        let episode_id = EpisodeId(episode_uuid);

        // Resolve the enclosure URL + whether it is already on disk. A
        // `file://` URL or a bare absolute path is local (Downloaded); an
        // `http(s)://` URL is a remote enclosure (NotDownloaded).
        let is_local = enclosure_url.starts_with("file://") || enclosure_url.starts_with('/');
        let parsed_url = if is_local {
            match path_to_file_url(enclosure_url) {
                Some(u) => u,
                None => return false,
            }
        } else {
            match url::Url::parse(enclosure_url) {
                Ok(u) => u,
                Err(_) => return false,
            }
        };

        // Episode::new derives a feed-and-guid id; override with the
        // Swift-minted UUID so it is the stable key everything else (positions,
        // local paths, publish lookups) is keyed off.
        let mut episode = Episode::new(
            podcast_id,
            "agent-generated://podcast",
            episode_id_str,
            title,
            parsed_url.clone(),
            chrono::Utc::now(),
        );
        episode.id = episode_id;
        episode.description = description;
        episode.duration_secs = duration_secs;
        // Explicit `image_url` wins; otherwise inherit the first chapter image.
        episode.image_url = image_url
            .as_deref()
            .and_then(|u| url::Url::parse(u).ok())
            .or_else(|| {
                chapters
                    .iter()
                    .find_map(|c| c.image_url.as_deref())
                    .and_then(|u| url::Url::parse(u).ok())
            });
        if !chapters.is_empty() {
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
        }

        // Local file: wire for immediate playback (Downloaded + local path +
        // mp4 mime). Remote enclosure: leave NotDownloaded for the download
        // capability to fetch later.
        let byte_count = if is_local {
            let path = file_url_to_path(&parsed_url);
            let bytes = path
                .as_deref()
                .and_then(|p| std::fs::metadata(p).ok())
                .map(|m| m.len() as i64)
                .unwrap_or(0);
            episode.enclosure_mime_type = Some("audio/mp4".to_string());
            episode.download_state = DownloadState::Downloaded {
                local_file_url: parsed_url.clone(),
                byte_count: bytes,
            };
            Some((path, bytes))
        } else {
            None
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
        if let Some((Some(path), bytes)) = byte_count {
            self.set_local_path(episode_id, path, bytes);
        }
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
        self.notifications_disabled.remove(&id);
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
    pub fn episode_with_podcast_clone(&self, episode_id_str: &str) -> Option<(Podcast, Episode)> {
        for (podcast_id, episodes) in &self.episodes {
            if let Some(ep) = episodes
                .iter()
                .find(|e| e.id.0.to_string() == episode_id_str)
            {
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

/// Resolve the local filesystem path from a `file://` [`url::Url`]. Returns
/// `None` when the URL is not a usable file path. Used to populate the
/// `local_paths` side-map (which is keyed by raw path, not URL) for a
/// locally-stored episode.
fn file_url_to_path(file_url: &url::Url) -> Option<String> {
    file_url
        .to_file_path()
        .ok()
        .and_then(|p| p.to_str().map(str::to_owned))
}

#[cfg(test)]
#[path = "owned_ext_tests.rs"]
mod tests;
