//! iTunes Search API helpers — URL encoding + JSON response decoding.
//!
//! Hoisted out of `host_op_handler.rs` to keep that file under the
//! 500-LOC ceiling. The shape and behaviour are unchanged from the
//! original inline implementation.

use crate::ffi::projections::PodcastSummary;

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

/// Parse the iTunes Search API JSON payload into `PodcastSummary` rows.
/// Returns an empty Vec on any decode failure (D6).
pub(crate) fn parse_itunes_results(body: &str) -> Vec<PodcastSummary> {
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
        artist_name: Option<String>,
    }
    let Ok(resp) = serde_json::from_str::<ItunesResponse>(body) else {
        return vec![];
    };
    resp.results
        .into_iter()
        .filter_map(|r| {
            Some(PodcastSummary {
                id: r.collection_id?.to_string(),
                title: r.collection_name.unwrap_or_default(),
                episode_count: 0,
                unplayed_count: 0,
                artwork_url: r.artwork_url600,
                feed_url: r.feed_url,
                author: r.artist_name,
                auto_download: false,
                episodes: vec![],
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_encode_passes_through_unreserved() {
        assert_eq!(url_encode("AZaz09-_.~"), "AZaz09-_.~");
    }

    #[test]
    fn url_encode_converts_space_to_plus() {
        assert_eq!(url_encode("a b c"), "a+b+c");
    }

    #[test]
    fn url_encode_percent_encodes_other_chars() {
        assert_eq!(url_encode("!?"), "%21%3F");
    }

    #[test]
    fn parse_itunes_results_returns_empty_on_garbage() {
        assert_eq!(parse_itunes_results("not json"), Vec::<PodcastSummary>::new());
    }

    #[test]
    fn parse_itunes_results_decodes_minimal_response() {
        let body = r#"{
            "results": [{
                "collectionId": 1234567,
                "collectionName": "Some Show",
                "feedUrl": "https://feed.example.com/r.rss",
                "artworkUrl600": "https://img.example.com/c.jpg",
                "artistName": "Host Name"
            }]
        }"#;
        let out = parse_itunes_results(body);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "1234567");
        assert_eq!(out[0].title, "Some Show");
        assert_eq!(out[0].feed_url.as_deref(), Some("https://feed.example.com/r.rss"));
        assert_eq!(out[0].author.as_deref(), Some("Host Name"));
    }
}
