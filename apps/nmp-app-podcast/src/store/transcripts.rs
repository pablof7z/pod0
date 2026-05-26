//! Episode → transcript-source accessor.
//!
//! The transcript *cache* (parsed entries keyed by episode id) lives on
//! `PodcastHandle` next to `search_results` — transcripts are transient,
//! per-session state owned by the FFI handle rather than the persisted
//! `PodcastStore`. This module only exposes the lookup the host op handler
//! needs to discover *where* to fetch the transcript bytes from, plus what
//! parser to use once the bytes arrive.

use podcast_core::TranscriptKind;

use super::PodcastStore;

impl PodcastStore {
    /// Resolve an episode UUID string to its publisher transcript URL +
    /// declared format.
    ///
    /// Returns `None` when the episode is unknown or when the RSS feed did
    /// not advertise a `<podcast:transcript>` tag. The kind defaults to
    /// `TranscriptKind::Json` (Podcasting 2.0) when the publisher URL is
    /// present but the `type` attribute is missing — that's the most common
    /// shape in the wild.
    /// Return the cached raw-text transcript for `id_str`, if one has been
    /// stored via the transcript write path.
    pub fn transcript_for(&self, id_str: &str) -> Option<&str> {
        self.transcripts.get(id_str).map(String::as_str)
    }

    /// Store a raw-text transcript for `episode_id_str`. Overwrites any
    /// previously cached transcript for the same id.
    pub fn set_transcript(&mut self, episode_id_str: String, text: String) {
        self.transcripts.insert(episode_id_str, text);
    }

    pub fn episode_publisher_transcript(&self, id_str: &str) -> Option<(String, TranscriptKind)> {
        for episodes in self.episodes.values() {
            if let Some(ep) = episodes.iter().find(|e| e.id.0.to_string() == id_str) {
                let url = ep.publisher_transcript_url.as_ref()?;
                let kind = ep
                    .publisher_transcript_type
                    .unwrap_or(TranscriptKind::Json);
                return Some((url.to_string(), kind));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use podcast_core::{Episode, Podcast, TranscriptKind};

    fn make_episode(podcast_id: podcast_core::PodcastId) -> Episode {
        let mut episode = Episode::new(
            podcast_id,
            "https://example.com/feed.xml",
            "guid-transcript",
            "Transcript Episode",
            url::Url::parse("https://example.com/audio.mp3").unwrap(),
            chrono::Utc::now(),
        );
        episode.publisher_transcript_url =
            Some(url::Url::parse("https://example.com/transcript.vtt").unwrap());
        episode.publisher_transcript_type = Some(TranscriptKind::Vtt);
        episode
    }

    #[test]
    fn episode_publisher_transcript_returns_url_and_kind() {
        let mut store = PodcastStore::new();
        let podcast = Podcast::new("Transcript Show");
        let episode = make_episode(podcast.id);
        let id = episode.id.0.to_string();
        store.subscribe(podcast, vec![episode]);

        let (url, kind) = store.episode_publisher_transcript(&id).expect("transcript info");
        assert_eq!(url, "https://example.com/transcript.vtt");
        assert_eq!(kind, TranscriptKind::Vtt);
    }

    #[test]
    fn episode_publisher_transcript_returns_none_when_no_url() {
        let mut store = PodcastStore::new();
        let podcast = Podcast::new("No Transcript Show");
        let episode = Episode::new(
            podcast.id,
            "https://example.com/feed.xml",
            "guid",
            "Episode",
            url::Url::parse("https://example.com/audio.mp3").unwrap(),
            chrono::Utc::now(),
        );
        let id = episode.id.0.to_string();
        store.subscribe(podcast, vec![episode]);

        assert!(store.episode_publisher_transcript(&id).is_none());
    }

    #[test]
    fn episode_publisher_transcript_defaults_kind_to_json() {
        let mut store = PodcastStore::new();
        let podcast = Podcast::new("Show");
        let mut episode = Episode::new(
            podcast.id,
            "https://example.com/feed.xml",
            "guid",
            "Episode",
            url::Url::parse("https://example.com/audio.mp3").unwrap(),
            chrono::Utc::now(),
        );
        episode.publisher_transcript_url =
            Some(url::Url::parse("https://example.com/transcript").unwrap());
        episode.publisher_transcript_type = None;
        let id = episode.id.0.to_string();
        store.subscribe(podcast, vec![episode]);

        let (_url, kind) = store.episode_publisher_transcript(&id).expect("info");
        assert_eq!(kind, TranscriptKind::Json);
    }
}
