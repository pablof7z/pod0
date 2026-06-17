//! iTunes Search API helpers — URL encoding + JSON response decoding.
//!
//! Hoisted out of `host_op_handler.rs` to keep that file under the
//! 500-LOC ceiling. The shape and behaviour are unchanged from the
//! original inline implementation.

use crate::ffi::projections::PodcastSummary;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ItunesSearchKind {
    Podcast,
    Episode,
}

impl ItunesSearchKind {
    pub(crate) fn from_str(raw: &str) -> Option<Self> {
        match raw {
            "podcast" => Some(Self::Podcast),
            "episode" | "podcastEpisode" => Some(Self::Episode),
            _ => None,
        }
    }

    fn entity(self) -> &'static str {
        match self {
            Self::Podcast => "podcast",
            Self::Episode => "podcastEpisode",
        }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub(crate) struct ItunesDirectoryHit {
    pub collection_id: Option<i64>,
    pub podcast_title: String,
    pub author: Option<String>,
    pub feed_url: Option<String>,
    pub artwork_url: Option<String>,
    pub primary_genre_name: Option<String>,
    pub track_count: Option<i64>,
    pub episode_title: Option<String>,
    pub episode_audio_url: Option<String>,
    pub episode_guid: Option<String>,
    pub episode_published_at: Option<i64>,
    pub episode_duration_seconds: Option<i64>,
    pub episode_description: Option<String>,
}

/// Percent-encode a query string for the iTunes search URL.
///
/// Unreserved characters (alphanumerics + `-`, `_`, `.`, `~`) pass
/// through. Spaces become `+`. Everything else is encoded as
/// uppercase `%HH` per byte.
pub(crate) fn url_encode(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => {
                vec![c]
            }
            ' ' => vec!['+'],
            other => {
                let mut buf = [0u8; 4];
                let bytes = other.encode_utf8(&mut buf);
                bytes
                    .bytes()
                    .flat_map(|b| {
                        let hi = char::from_digit((b >> 4) as u32, 16).unwrap_or('0');
                        let lo = char::from_digit((b & 0xf) as u32, 16).unwrap_or('0');
                        vec!['%', hi.to_ascii_uppercase(), lo.to_ascii_uppercase()]
                    })
                    .collect()
            }
        })
        .collect()
}

pub(crate) fn search_url(query: &str, kind: ItunesSearchKind, limit: usize) -> String {
    let encoded = url_encode(query.trim());
    let limit = limit.clamp(1, 25);
    format!(
        "https://itunes.apple.com/search?term={encoded}&media=podcast&entity={}&limit={limit}&country=us&lang=en_us",
        kind.entity()
    )
}

pub(crate) fn lookup_url(collection_id: &str) -> Option<String> {
    let trimmed = collection_id.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(format!(
        "https://itunes.apple.com/lookup?id={}&entity=podcast",
        url_encode(trimmed)
    ))
}

pub(crate) fn top_podcasts_url(limit: usize, storefront: &str) -> String {
    let limit = limit.clamp(1, 25);
    let storefront = storefront.trim();
    let storefront = if storefront.is_empty() { "us" } else { storefront };
    format!(
        "https://rss.applemarketingtools.com/api/v2/{}/podcasts/top/{limit}/podcasts.json",
        url_encode(storefront)
    )
}

pub(crate) fn lookup_ids_url(ids: &[i64]) -> Option<String> {
    if ids.is_empty() {
        return None;
    }
    let joined = ids
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    Some(format!(
        "https://itunes.apple.com/lookup?id={joined}&entity=podcast"
    ))
}

/// Parse the iTunes Search API JSON payload into `PodcastSummary` rows.
/// Returns an empty Vec on any decode failure (D6).
pub(crate) fn parse_itunes_results(body: &str) -> Vec<PodcastSummary> {
    parse_itunes_directory_results(body, ItunesSearchKind::Podcast)
        .into_iter()
        .filter_map(|r| {
            Some(PodcastSummary {
                id: r.collection_id?.to_string(),
                title: r.podcast_title,
                episode_count: 0,
                unplayed_count: 0,
                artwork_url: r.artwork_url,
                feed_url: r.feed_url,
                author: r.author,
                description: None,
                last_refreshed_at: None,
                title_is_placeholder: false,
                // iTunes search rows are feed-backed shows the user has not
                // subscribed to — no owner, default visibility.
                is_subscribed: false,
                owner_pubkey_hex: None,
                nostr_visibility: "public".to_string(),
                auto_download: false,
                auto_download_mode: String::new(),
                auto_download_count: 0,
                cellular_allowed: false,
                notifications_enabled: true,
                // iTunes search rows have no user-curated categories.
                user_categories: Vec::new(),
                // iTunes search rows default to transcription enabled.
                transcription_enabled: true,
                episodes: vec![],
            })
        })
        .collect()
}

pub(crate) fn parse_itunes_directory_results(
    body: &str,
    kind: ItunesSearchKind,
) -> Vec<ItunesDirectoryHit> {
    #[derive(serde::Deserialize)]
    struct ItunesResponse {
        results: Vec<ItunesResult>,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ItunesResult {
        collection_id: Option<i64>,
        collection_name: Option<String>,
        feed_url: Option<String>,
        artwork_url600: Option<String>,
        artwork_url100: Option<String>,
        artist_name: Option<String>,
        primary_genre_name: Option<String>,
        track_count: Option<i64>,
        track_name: Option<String>,
        episode_url: Option<String>,
        episode_guid: Option<String>,
        release_date: Option<String>,
        track_time_millis: Option<i64>,
        description: Option<String>,
    }
    let Ok(resp) = serde_json::from_str::<ItunesResponse>(body) else {
        return vec![];
    };
    resp.results
        .into_iter()
        .filter_map(|r| {
            let podcast_title = r
                .collection_name
                .clone()
                .or_else(|| r.artist_name.clone())
                .unwrap_or_default();
            if podcast_title.is_empty() {
                return None;
            }
            let feed_url = r.feed_url.filter(|url| !url.trim().is_empty())?;
            let episode_published_at = r
                .release_date
                .as_deref()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|d| d.timestamp());
            Some(ItunesDirectoryHit {
                collection_id: r.collection_id,
                podcast_title,
                author: r.artist_name,
                feed_url: Some(feed_url),
                artwork_url: r.artwork_url600.or(r.artwork_url100),
                primary_genre_name: r.primary_genre_name,
                track_count: r.track_count,
                episode_title: (kind == ItunesSearchKind::Episode).then_some(r.track_name).flatten(),
                episode_audio_url: (kind == ItunesSearchKind::Episode)
                    .then_some(r.episode_url)
                    .flatten(),
                episode_guid: (kind == ItunesSearchKind::Episode)
                    .then_some(r.episode_guid)
                    .flatten(),
                episode_published_at: (kind == ItunesSearchKind::Episode)
                    .then_some(episode_published_at)
                    .flatten(),
                episode_duration_seconds: (kind == ItunesSearchKind::Episode)
                    .then_some(r.track_time_millis.map(|ms| ms / 1_000))
                    .flatten(),
                episode_description: (kind == ItunesSearchKind::Episode)
                    .then_some(r.description)
                    .flatten(),
            })
        })
        .collect()
}

pub(crate) fn parse_lookup_feed_url(body: &str) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct ItunesResponse {
        results: Vec<LookupResult>,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct LookupResult {
        feed_url: Option<String>,
    }
    let resp = serde_json::from_str::<ItunesResponse>(body).ok()?;
    resp.results.into_iter().find_map(|r| {
        let feed_url = r.feed_url?;
        (!feed_url.is_empty()).then_some(feed_url)
    })
}

pub(crate) fn parse_top_podcast_ids(body: &str) -> Vec<i64> {
    #[derive(serde::Deserialize)]
    struct TopPodcastsFeed {
        feed: TopPodcastsBody,
    }
    #[derive(serde::Deserialize)]
    struct TopPodcastsBody {
        results: Vec<TopPodcastsItem>,
    }
    #[derive(serde::Deserialize)]
    struct TopPodcastsItem {
        id: String,
    }
    let Ok(resp) = serde_json::from_str::<TopPodcastsFeed>(body) else {
        return Vec::new();
    };
    resp.feed
        .results
        .into_iter()
        .filter_map(|item| item.id.parse::<i64>().ok())
        .collect()
}

pub(crate) fn order_hits_by_rank(
    hits: Vec<ItunesDirectoryHit>,
    ranked_ids: &[i64],
) -> Vec<ItunesDirectoryHit> {
    let mut by_id = std::collections::HashMap::new();
    for hit in hits {
        if let Some(id) = hit.collection_id {
            if hit.feed_url.as_deref().is_some_and(|feed| !feed.is_empty()) {
                by_id.insert(id, hit);
            }
        }
    }
    ranked_ids
        .iter()
        .filter_map(|id| by_id.remove(id))
        .collect()
}

#[cfg(test)]
#[path = "itunes_tests.rs"]
mod tests;
