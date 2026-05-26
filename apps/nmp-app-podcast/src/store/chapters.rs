use podcast_core::Chapter;

use super::PodcastStore;

impl PodcastStore {
    pub fn episode_chapters_state(&self, id_str: &str) -> Option<(Option<url::Url>, bool)> {
        for episodes in self.episodes.values() {
            if let Some(ep) = episodes.iter().find(|e| e.id.0.to_string() == id_str) {
                let loaded = ep.chapters.as_ref().map(|c| !c.is_empty()).unwrap_or(false);
                return Some((ep.chapters_url.clone(), loaded));
            }
        }
        None
    }

    /// Look up the published duration of an episode (`Episode::duration_secs`).
    ///
    /// Returns `None` when the episode is missing or when the RSS feed did
    /// not advertise a duration. The AI chapter compiler ([`crate::ai_chapters`])
    /// gates on this — without a duration there's no way to compute
    /// equally-spaced chapter offsets.
    pub fn episode_duration_secs(&self, id_str: &str) -> Option<f64> {
        for episodes in self.episodes.values() {
            if let Some(ep) = episodes.iter().find(|e| e.id.0.to_string() == id_str) {
                return ep.duration_secs;
            }
        }
        None
    }

    pub fn set_episode_chapters(&mut self, id_str: &str, chapters: Vec<Chapter>) -> bool {
        for episodes in self.episodes.values_mut() {
            if let Some(ep) = episodes.iter_mut().find(|e| e.id.0.to_string() == id_str) {
                ep.chapters = if chapters.is_empty() { None } else { Some(chapters) };
                self.persist();
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use podcast_core::{Episode, Podcast};

    #[test]
    fn set_episode_chapters_replaces_existing_list() {
        let mut store = PodcastStore::new();
        let mut podcast = Podcast::new("Show");
        let podcast_id = podcast.id;
        podcast.feed_url = None;
        let mut ep = Episode::new(
            podcast_id,
            "https://example.com/feed.xml",
            "guid-1",
            "Ep",
            url::Url::parse("https://example.com/e.mp3").unwrap(),
            chrono::Utc::now(),
        );
        let ep_id = ep.id.0.to_string();
        ep.chapters_url = Some(url::Url::parse("https://example.com/chapters.json").unwrap());
        store.subscribe(podcast, vec![ep]);

        let (url, loaded) = store.episode_chapters_state(&ep_id).unwrap();
        assert!(url.is_some());
        assert!(!loaded);

        let chapters = vec![Chapter::new("Intro", 0.0), Chapter::new("Outro", 60.0)];
        assert!(store.set_episode_chapters(&ep_id, chapters));
        let (_url, loaded) = store.episode_chapters_state(&ep_id).unwrap();
        assert!(loaded);
        assert!(!store.set_episode_chapters("missing", vec![]));
    }

    #[test]
    fn episode_duration_secs_returns_published_duration() {
        let mut store = PodcastStore::new();
        let podcast = Podcast::new("Show");
        let podcast_id = podcast.id;
        let mut ep = Episode::new(
            podcast_id,
            "https://example.com/feed.xml",
            "guid-1",
            "Ep",
            url::Url::parse("https://example.com/e.mp3").unwrap(),
            chrono::Utc::now(),
        );
        let ep_id = ep.id.0.to_string();
        ep.duration_secs = Some(1800.0);
        store.subscribe(podcast, vec![ep]);
        assert_eq!(store.episode_duration_secs(&ep_id), Some(1800.0));
        assert_eq!(store.episode_duration_secs("missing"), None);
    }
}
