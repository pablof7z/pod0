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
                description: None,
                // iTunes search rows are always RSS shows the user has not
                // subscribed to — no owner, default visibility.
                kind: "rss".to_string(),
                owner_pubkey_hex: None,
                nostr_visibility: "public".to_string(),
                auto_download: false,
                cellular_allowed: false,
                episodes: vec![],
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "itunes_tests.rs"]
mod tests;
