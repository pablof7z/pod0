use pod0_domain::{
    EpisodeRecord, FeedIdentityV1, PodcastId, PodcastKind, PodcastRecord,
    UnixTimestampMilliseconds, make_feed_identity_v1,
};
use url::Url;

use crate::feed_parser_reader::parse_rss;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedPodcastFeed {
    pub podcast: PodcastRecord,
    pub episodes: Vec<EpisodeRecord>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedParseFailure {
    InvalidUrl,
    MalformedXml,
    MissingChannel,
}

#[must_use]
pub fn normalize_feed_url(input: &str) -> Option<FeedIdentityV1> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let candidate = if trimmed.contains("://") {
        trimmed.to_owned()
    } else {
        format!("https://{trimmed}")
    };
    let parsed = Url::parse(&candidate).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return None;
    }
    make_feed_identity_v1(candidate).ok()
}

pub fn parse_podcast_feed(
    bytes: &[u8],
    feed_identity: FeedIdentityV1,
    podcast_id: PodcastId,
    observed_at: UnixTimestampMilliseconds,
) -> Result<ParsedPodcastFeed, FeedParseFailure> {
    let base_url =
        Url::parse(&feed_identity.source_url).map_err(|_| FeedParseFailure::InvalidUrl)?;
    let parsed = parse_rss(bytes, &base_url, podcast_id)?;
    Ok(ParsedPodcastFeed {
        podcast: PodcastRecord {
            podcast_id,
            kind: PodcastKind::Rss,
            feed_identity: Some(feed_identity),
            title: parsed.title,
            author: parsed.author,
            image_url: parsed.image_url,
            description: parsed.description,
            language: parsed.language,
            categories: parsed.categories,
            discovered_at: observed_at,
            title_is_placeholder: false,
            last_refreshed_at: Some(observed_at),
            etag: None,
            last_modified: None,
        },
        episodes: parsed.episodes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_matches_the_versioned_swift_comparison_rule() {
        let identity = normalize_feed_url(" EXAMPLE.test/Feed ").unwrap();
        assert_eq!(identity.source_url, "https://EXAMPLE.test/Feed");
        assert_eq!(identity.comparison_key, "https://example.test/feed");
        assert!(normalize_feed_url("file:///tmp/feed").is_none());
        assert!(normalize_feed_url("https://").is_none());
    }
}
