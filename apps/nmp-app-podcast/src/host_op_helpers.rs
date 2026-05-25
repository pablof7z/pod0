//! Free-function helpers extracted from
//! [`crate::host_op_handler::PodcastHostOpHandler`] so the main file
//! stays under the 500-line hard limit. None of these touch the
//! handler's state — they're pure transforms exercised by the
//! handler's methods.

use podcast_core::Episode;

use crate::ffi::projections::PodcastSummary;

/// Preserve per-episode `position_secs` across a feed refresh.
///
/// `fresh` is the parser's output; `existing` is what the store
/// currently has. Matched by `Episode::id`. Note that the wider
/// pipeline currently regenerates `EpisodeId` as a random `Uuid` on
/// every parse (see `Episode::new`), which means this match rarely
/// fires in practice — tracked as a follow-up; this helper keeps the
/// shape stable so the fix lands in one place.
pub(crate) fn merge_episodes(fresh: Vec<Episode>, existing: Vec<Episode>) -> Vec<Episode> {
    fresh
        .into_iter()
        .map(|mut ep| {
            if let Some(prev) = existing.iter().find(|e| e.id == ep.id) {
                ep.position_secs = prev.position_secs;
            }
            ep
        })
        .collect()
}

/// `application/x-www-form-urlencoded`-style encoder used to build
/// the iTunes Search API query string. Kept local so we don't pull
/// in a heavy dependency for one call site.
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
                bytes.bytes().flat_map(|b| {
                    let hi = char::from_digit((b >> 4) as u32, 16).unwrap_or('0');
                    let lo = char::from_digit((b & 0xf) as u32, 16).unwrap_or('0');
                    vec!['%', hi.to_ascii_uppercase(), lo.to_ascii_uppercase()]
                }).collect()
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
