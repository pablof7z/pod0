//! iTunes Search API helpers — extracted from `host_op_handler.rs` to keep
//! that file under the 500-LOC hard ceiling once voice-mode wiring landed.
//!
//! Two surfaces:
//!
//! * [`url_encode`] — RFC-3986-style query-component encoder used to build the
//!   `term=` parameter of the iTunes Search URL. Hand-rolled rather than
//!   pulled from `url::form_urlencoded` to keep the dependency surface narrow
//!   and to encode `' '` as `+` (the iTunes endpoint accepts both `%20` and
//!   `+`, but the test corpus pins `+`).
//! * [`parse_itunes_results`] — turns the raw iTunes JSON body into the
//!   shell-facing [`PodcastSummary`] projection. Returns an empty vector on
//!   any decode failure (D6 — degrade silently).

use crate::ffi::projections::PodcastSummary;

/// Encode `s` as an `application/x-www-form-urlencoded` query value:
/// alphanumerics + `-_.~` pass through, spaces become `+`, everything else
/// becomes a UTF-8 `%xx` sequence in upper hex.
pub(crate) fn url_encode(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => vec![c],
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
                episodes: vec![],
            })
        })
        .collect()
}
