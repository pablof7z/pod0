use std::collections::HashMap;

use serde::Deserialize;
use url::Url;

use crate::{LibraryHttpRequest, PodcastDirectoryEntry};

#[derive(Debug, PartialEq, Eq)]
pub enum LibraryDirectoryError {
    InvalidInput,
    InvalidResponse,
}

#[must_use]
pub fn plan_directory_search(term: &str, limit: u16) -> Option<LibraryHttpRequest> {
    let term = term.trim();
    if term.is_empty() {
        return None;
    }
    let mut url = Url::parse("https://itunes.apple.com/search").ok()?;
    url.query_pairs_mut()
        .append_pair("media", "podcast")
        .append_pair("entity", "podcast")
        .append_pair("term", term)
        .append_pair("limit", &limit.clamp(1, 50).to_string());
    Some(json_request(url))
}

#[must_use]
pub fn plan_top_chart(storefront: &str, limit: u16) -> Option<LibraryHttpRequest> {
    let storefront = storefront.trim().to_ascii_lowercase();
    if storefront.len() != 2 || !storefront.bytes().all(|value| value.is_ascii_lowercase()) {
        return None;
    }
    let limit = limit.clamp(1, 50);
    let url = Url::parse(&format!(
        "https://rss.applemarketingtools.com/api/v2/{storefront}/podcasts/top/{limit}/podcasts.json"
    ))
    .ok()?;
    Some(json_request(url))
}

#[must_use]
pub fn plan_directory_lookup(ids: &[u64]) -> Option<LibraryHttpRequest> {
    if ids.is_empty() || ids.len() > 50 {
        return None;
    }
    let mut url = Url::parse("https://itunes.apple.com/lookup").ok()?;
    url.query_pairs_mut()
        .append_pair(
            "id",
            &ids.iter().map(u64::to_string).collect::<Vec<_>>().join(","),
        )
        .append_pair("entity", "podcast");
    Some(json_request(url))
}

pub fn parse_directory_response(
    bytes: &[u8],
) -> Result<Vec<PodcastDirectoryEntry>, LibraryDirectoryError> {
    let response: DirectoryResponse =
        serde_json::from_slice(bytes).map_err(|_| LibraryDirectoryError::InvalidResponse)?;
    Ok(response
        .results
        .into_iter()
        .filter_map(DirectoryRow::normalized)
        .collect())
}

pub fn parse_top_chart_ids(bytes: &[u8]) -> Result<Vec<u64>, LibraryDirectoryError> {
    let response: TopResponse =
        serde_json::from_slice(bytes).map_err(|_| LibraryDirectoryError::InvalidResponse)?;
    Ok(response
        .feed
        .results
        .into_iter()
        .filter_map(|row| row.id.parse().ok())
        .take(50)
        .collect())
}

pub fn order_directory_results(
    entries: Vec<PodcastDirectoryEntry>,
    ranked_ids: &[u64],
) -> Vec<PodcastDirectoryEntry> {
    let mut entries: HashMap<_, _> = entries
        .into_iter()
        .map(|entry| (entry.collection_id, entry))
        .collect();
    ranked_ids
        .iter()
        .filter_map(|id| entries.remove(id))
        .collect()
}

fn json_request(url: Url) -> LibraryHttpRequest {
    LibraryHttpRequest {
        url: url.into(),
        accept: "application/json".into(),
        maximum_response_bytes: 1_000_000,
    }
}

#[derive(Deserialize)]
struct DirectoryResponse {
    results: Vec<DirectoryRow>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DirectoryRow {
    collection_id: u64,
    collection_name: String,
    artist_name: Option<String>,
    feed_url: Option<String>,
    artwork_url600: Option<String>,
    artwork_url100: Option<String>,
    primary_genre_name: Option<String>,
    track_count: Option<u32>,
}

impl DirectoryRow {
    fn normalized(self) -> Option<PodcastDirectoryEntry> {
        let feed_url = valid_web_url(self.feed_url.as_deref()?)?;
        let artwork_url = self
            .artwork_url600
            .as_deref()
            .and_then(valid_web_url)
            .or_else(|| self.artwork_url100.as_deref().and_then(valid_web_url));
        Some(PodcastDirectoryEntry {
            collection_id: self.collection_id,
            collection_name: self.collection_name,
            artist_name: self.artist_name,
            feed_url,
            artwork_url,
            primary_genre_name: self.primary_genre_name,
            track_count: self.track_count,
        })
    }
}

fn valid_web_url(value: &str) -> Option<String> {
    let url = Url::parse(value).ok()?;
    matches!(url.scheme(), "http" | "https").then(|| url.into())
}

#[derive(Deserialize)]
struct TopResponse {
    feed: TopBody,
}

#[derive(Deserialize)]
struct TopBody {
    results: Vec<TopRow>,
}

#[derive(Deserialize)]
struct TopRow {
    id: String,
}
