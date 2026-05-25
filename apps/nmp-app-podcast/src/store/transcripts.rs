use podcast_core::TranscriptKind;

use super::PodcastStore;

impl PodcastStore {
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

    pub fn transcript_for(&self, id_str: &str) -> Option<&str> {
        self.transcripts.get(id_str).map(String::as_str)
    }

    pub fn set_transcript(&mut self, id_str: impl Into<String>, text: impl Into<String>) {
        self.transcripts.insert(id_str.into(), text.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use podcast_core::{Episode, Podcast, TranscriptKind};

    fn make_episode() -> Episode {
        let podcast = Podcast::new("Transcript Show");
        let mut episode = Episode::new(
            podcast.id,
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
    fn transcript_round_trip() {
        let mut store = PodcastStore::new();
        assert!(store.transcript_for("ep-1").is_none());
        store.set_transcript("ep-1", "Hello world.");
        assert_eq!(store.transcript_for("ep-1"), Some("Hello world."));
        store.set_transcript("ep-1", "Updated transcript.");
        assert_eq!(store.transcript_for("ep-1"), Some("Updated transcript."));
    }

    #[test]
    fn episode_publisher_transcript_returns_url_and_kind() {
        let mut store = PodcastStore::new();
        let podcast = Podcast::new("Transcript Show");
        let mut episode = make_episode();
        let id = episode.id.0.to_string();
        episode.podcast_id = podcast.id;
        store.subscribe(podcast, vec![episode]);

        let (url, kind) = store.episode_publisher_transcript(&id).expect("transcript info");
        assert_eq!(url, "https://example.com/transcript.vtt");
        assert_eq!(kind, TranscriptKind::Vtt);
    }
}
