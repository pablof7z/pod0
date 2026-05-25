use podcast_core::PodcastId;
use serde::{Deserialize, Serialize};
use url::Url;

/// Action: refresh a single feed by URL. The podcast identity is preserved
/// across the round trip via `podcast_id` so the dispatcher can correlate
/// the result back into the store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefreshFeedAction {
    pub podcast_id: PodcastId,
    pub feed_url: Url,
}

/// Action: refresh every followed feed. The dispatcher applies the
/// `RefreshPolicy` per-subscription to decide which feeds are due.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefreshAllFeedsAction;

/// Action: import an OPML XML document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportOpmlAction {
    pub opml_xml: String,
}

/// Action: export current followed podcasts as OPML. The dispatcher
/// collects the podcast list and calls `export_opml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportOpmlAction;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_feed_action_round_trip() {
        let action = RefreshFeedAction {
            podcast_id: PodcastId::generate(),
            feed_url: Url::parse("https://example.com/feed.xml").unwrap(),
        };
        let json = serde_json::to_string(&action).unwrap();
        let back: RefreshFeedAction = serde_json::from_str(&json).unwrap();
        assert_eq!(action, back);
    }

    #[test]
    fn import_opml_action_round_trip() {
        let action = ImportOpmlAction {
            opml_xml: "<opml/>".into(),
        };
        let json = serde_json::to_string(&action).unwrap();
        let back: ImportOpmlAction = serde_json::from_str(&json).unwrap();
        assert_eq!(action, back);
    }
}
