//! NIP-73 anchor helpers for episode comments.
//!
//! The anchor format used by NIP-22 (kind 1111) for podcast episodes is the
//! Podcasting 2.0 URI scheme: `podcast:item:guid:<rss_guid>`. This matches
//! the original Swift `NostrCommentService` which anchors via the same scheme.
//!
//! See NIP-73 §1 ("External content identifiers") for the spec.

use crate::store::PodcastStore;

/// Build the NIP-73 `podcast:item:guid:<guid>` anchor for an episode.
///
/// Returns `None` when the episode is not found in the store. Falls back to
/// the episode UUID string when the RSS `guid` field is empty (edge case for
/// synthetic episodes that lack a canonical feed guid).
pub fn episode_nip73_anchor(store: &PodcastStore, episode_id: &str) -> Option<String> {
    let (_, episode) = store.episode_with_podcast_clone(episode_id)?;
    let guid = if !episode.guid.is_empty() {
        episode.guid.clone()
    } else {
        episode.id.0.to_string()
    };
    Some(format!("podcast:item:guid:{guid}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use podcast_core::{Episode, EpisodeId, Podcast};
    use crate::store::PodcastStore;
    use chrono::Utc;
    use url::Url;

    fn make_store_with_episode(guid: &str) -> (PodcastStore, String) {
        let mut store = PodcastStore::new();
        let feed_url = "http://example.com/feed.xml";
        let podcast = Podcast::new("Test Podcast");
        let podcast_id = podcast.id;
        let ep_id = EpisodeId::from_feed_and_guid(feed_url, guid);
        let episode = Episode::new(
            podcast_id,
            feed_url,
            guid,
            "Test Episode",
            Url::parse("http://example.com/ep1.mp3").unwrap(),
            Utc::now(),
        );
        let ep_id_str = ep_id.0.to_string();
        store.subscribe(podcast, vec![episode]);
        (store, ep_id_str)
    }

    #[test]
    fn anchor_uses_guid_when_present() {
        let (store, ep_id_str) = make_store_with_episode("my-episode-guid-123");
        let anchor = episode_nip73_anchor(&store, &ep_id_str);
        assert_eq!(anchor, Some("podcast:item:guid:my-episode-guid-123".into()));
    }

    #[test]
    fn anchor_returns_none_for_missing_episode() {
        let store = PodcastStore::new();
        assert!(episode_nip73_anchor(&store, "no-such-id").is_none());
    }
}
