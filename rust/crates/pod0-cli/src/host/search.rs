use pod0_facade::HostObservation;
use reqwest::header::{ACCEPT, USER_AGENT};

use super::{HostExecutor, read_bounded};
use crate::protocol::{CliError, PodcastSearchResultDto};

const ITUNES_SEARCH_URL: &str = "https://itunes.apple.com/search";
const MAX_SEARCH_RESULTS: u16 = 200;
const MAX_SEARCH_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;
const USER_AGENT_VALUE: &str = concat!("pod0-cli/", env!("CARGO_PKG_VERSION"));

/// Search the iTunes podcast directory over real HTTP. Read-only: returns
/// directory metadata only. The caller subscribes to a chosen `feed_url`
/// through the durable `subscribe_feed` path. Nothing is mocked.
pub(crate) fn search(
    host: &HostExecutor,
    term: &str,
    requested_limit: u16,
) -> Result<Vec<PodcastSearchResultDto>, CliError> {
    let term = term.trim();
    if term.is_empty() {
        return Err(CliError::new("search_term_empty", "search term must not be empty", false));
    }
    let limit = requested_limit.clamp(1, MAX_SEARCH_RESULTS);
    let encoded = urlencode(term);
    let base = std::env::var("POD0_PODCAST_SEARCH_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| ITUNES_SEARCH_URL.to_owned());
    let url = format!(
        "{base}?media=podcast&limit={limit}&term={encoded}"
    );

    let response = host
        .client
        .get(&url)
        .header(USER_AGENT, USER_AGENT_VALUE)
        .header(ACCEPT, "application/json")
        .send()
        .map_err(|error| search_error(&error))?;
    let status = response.status();
    if !status.is_success() {
        return Err(CliError::new(
            "search_http_error",
            format!("iTunes search returned HTTP {status}"),
            status.is_server_error(),
        ));
    }
    let bytes = read_bounded(response, MAX_SEARCH_RESPONSE_BYTES)
        .map_err(|observation| observation_to_error(*observation))?;
    parse_results(&bytes)
}

fn parse_results(bytes: &[u8]) -> Result<Vec<PodcastSearchResultDto>, CliError> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| {
            CliError::new("search_invalid_json", "iTunes search returned invalid JSON", false)
        })?;
    let results = value
        .get("results")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            CliError::new("search_invalid_json", "iTunes search response had no results array", false)
        })?;
    let mut out = Vec::new();
    for item in results {
        let itunes_id = match item.get("collectionId").and_then(serde_json::Value::as_u64) {
            Some(id) => id,
            None => continue,
        };
        let title = item
            .get("trackName")
            .or_else(|| item.get("collectionName"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let author = item
            .get("artistName")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let feed_url = item
            .get("feedUrl")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let artwork_url = item
            .get("artworkUrl100")
            .or_else(|| item.get("artworkUrl600"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let track_count = item
            .get("trackCount")
            .and_then(serde_json::Value::as_u64)
            .and_then(|count| u32::try_from(count).ok());
        out.push(PodcastSearchResultDto {
            itunes_id,
            title,
            author,
            feed_url,
            artwork_url,
            track_count,
        });
    }
    Ok(out)
}

fn search_error(error: &reqwest::Error) -> CliError {
    if error.is_timeout() {
        CliError::new("search_timeout", "iTunes search request timed out", true)
    } else {
        CliError::new("search_offline", "iTunes search request failed", true)
    }
}

fn observation_to_error(observation: HostObservation) -> CliError {
    let detail = match observation {
        HostObservation::Failed { code, .. } => format!("iTunes search failed: {code:?}"),
        other => format!("iTunes search failed: {other:?}"),
    };
    CliError::new("search_http_error", detail, true)
}

/// Percent-encode a search term for a query string (RFC 3986 unreserved plus
/// the few safe query characters). Hand-rolled to avoid pulling in a new crate.
fn urlencode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.as_bytes() {
        let b = *byte;
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push('%');
            out.push_str(&format!("{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{parse_results, urlencode};

    #[test]
    fn urlencode_encodes_spaces_and_specials() {
        assert_eq!(urlencode("daily tech news"), "daily%20tech%20news");
        assert_eq!(urlencode("a&b=c"), "a%26b%3Dc");
        assert_eq!(urlencode("ok-_.~"), "ok-_.~");
    }

    #[test]
    fn parse_results_extracts_feed_url_and_skips_items_without_collection_id() {
        let body = r#"{
            "resultCount": 2,
            "results": [
                {"collectionId": 111,"trackName":"Show A","artistName":"Author A","feedUrl":"https://a/feed","artworkUrl100":"https://a/art","trackCount":50},
                {"trackName":"No id"},
                {"collectionId": 222,"collectionName":"Show B","artistName":"Author B","trackCount":12}
            ]
        }"#;
        let results = parse_results(body.as_bytes()).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].itunes_id, 111);
        assert_eq!(results[0].title, "Show A");
        assert_eq!(results[0].feed_url.as_deref(), Some("https://a/feed"));
        assert_eq!(results[0].track_count, Some(50));
        assert_eq!(results[1].title, "Show B");
        assert!(results[1].feed_url.is_none());
    }

    #[test]
    fn parse_results_rejects_responses_without_a_results_array() {
        assert!(parse_results(b"{}").is_err());
        assert!(parse_results(b"not json").is_err());
    }
}