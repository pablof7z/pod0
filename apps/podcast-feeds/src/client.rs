//! `FeedClient` — Rust orchestration that bridges the
//! [`crate::http`] capability and the streaming [`crate::rss::parse_feed`].
//!
//! Mirrors `App/Sources/Podcast/FeedClient.swift` one-for-one in semantics
//! but pure: it doesn't drive a `URLSession` itself, only builds the
//! [`HttpRequest`] the kernel hands to the iOS capability and interprets
//! the [`HttpResult`] that comes back.
//!
//! ## Why this is a separate module
//!
//! The RSS parser is encoding-agnostic ("here are some bytes, give me a
//! `ParsedFeed`"). The HTTP capability is transport-agnostic ("here is a
//! request, give me a result"). The feed-refresh pipeline is the bit that
//! knows the legacy conditional-GET protocol (`If-None-Match`,
//! `If-Modified-Since`) and decides what a `304` means. Keeping that
//! glue in its own module lets the parser and the capability stay narrow
//! while a future caller (M5.B, transcripts, OPML probe) can reuse the
//! request-building helper without re-implementing it.
//!
//! ## Doctrine
//!
//! * **D7 — pure decision logic.** No I/O. Build a request, project a
//!   response. The caller (kernel) routes it through the capability.
//! * **D9 — kernel owns time.** `last_refreshed` is supplied by the
//!   caller, not read from a wall clock here, so refresh decisions in
//!   tests are deterministic against an injected `now`.

use chrono::{DateTime, Utc};
use podcast_core::PodcastId;
use url::Url;

use crate::http::{HttpMethod, HttpRequest, HttpResult};
use crate::refresh::EtagCache;
use crate::rss::{parse_feed, ParseError, ParsedFeed};

/// `Accept` header sent on every feed request — matches the legacy Swift
/// `FeedClient.fetch` exactly so feed hosts that vary by Accept-mimetype
/// (a small but real population) see the same client behaviour after the
/// migration.
const ACCEPT_HEADER: &str = "application/rss+xml, application/xml;q=0.9, */*;q=0.8";

/// `User-Agent` sent on every feed request. The trailing version is bumped
/// only when the bridge shape changes; the leading `Podcastr/` is preserved
/// for feed-side analytics that already key off the legacy string.
const USER_AGENT: &str = "Podcastr/1.0";

/// Outcome of interpreting an [`HttpResult`] against the feed-refresh
/// protocol.
///
/// `PartialEq` is intentionally not derived: `ParsedFeed` carries
/// `chrono::DateTime`s for `discovered_at` / `last_refreshed_at` that the
/// parser stamps with `Utc::now()`, so two parses of the same body would
/// compare unequal. Tests match-pattern on this enum and assert on the
/// inner fields directly.
///
/// The variants are intentionally asymmetric in size (`Parsed` carries
/// the full [`ParsedFeed`]; `NotModified` is empty). Each refresh produces
/// exactly one [`FeedResult`] and the caller matches on it immediately, so
/// the wasted stack space on the `NotModified` arm is never paid in a hot
/// loop. If that ever changes, box `ParsedFeed`.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum FeedResult {
    /// Server returned `304 Not Modified`. The caller updates
    /// `last_refreshed` but does *not* touch the parsed-episode set.
    /// `cache` carries refreshed response validators, falling back to the
    /// existing ETag / Last-Modified when the response omits them.
    NotModified { cache: EtagCache },
    /// Server returned `200` with a body. The body parsed cleanly; the
    /// caller adopts the new podcast metadata + episodes and persists
    /// the refreshed `cache`.
    Parsed {
        feed_url: Url,
        parsed: ParsedFeed,
        cache: EtagCache,
    },
}

/// Errors the feed-refresh pipeline can surface to its caller. Each
/// variant carries enough context for the dispatcher to project the
/// failure into the user-facing diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedError {
    /// The iOS capability reported a transport-level failure (DNS, TLS,
    /// timeout, malformed request, capability stopped). The string is
    /// the raw `HttpResult::Error.message`.
    Transport(String),
    /// The server returned an HTTP status that isn't `200` or `304`.
    /// `status_code` is the raw HTTP status — interpretation is the
    /// caller's policy (subscribe-time 4xx surfaces as "feed not found";
    /// refresh-time 5xx surfaces as "try again later").
    Http { status_code: u16 },
    /// The body wasn't valid RSS. Wrapped to preserve the parser's
    /// distinction between "malformed XML" and "missing channel".
    Parse(ParseError),
}

impl From<ParseError> for FeedError {
    fn from(value: ParseError) -> Self {
        Self::Parse(value)
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Build the [`HttpRequest`] for a feed refresh.
///
/// Sends `If-None-Match` / `If-Modified-Since` when `cache` carries them,
/// so the server can short-circuit with `304`. Matches the legacy Swift
/// `FeedClient.fetch` header set exactly (Accept, User-Agent, conditional
/// headers in the same order); see `App/Sources/Podcast/FeedClient.swift`.
///
/// `feed_url` is consumed by value to make the call site obvious that this
/// is the URL the executor will hit — callers normally `.clone()` from
/// `Podcast.feed_url`.
#[must_use]
pub fn build_feed_request(feed_url: &Url, cache: Option<&EtagCache>) -> HttpRequest {
    let mut headers: Vec<Vec<String>> = vec![
        vec!["Accept".into(), ACCEPT_HEADER.into()],
        vec!["User-Agent".into(), USER_AGENT.into()],
    ];
    if let Some(cache) = cache {
        if let Some(etag) = cache.etag.as_deref().filter(|s| !s.is_empty()) {
            headers.push(vec!["If-None-Match".into(), etag.into()]);
        }
        if let Some(last_modified) = cache.last_modified.as_deref().filter(|s| !s.is_empty()) {
            headers.push(vec!["If-Modified-Since".into(), last_modified.into()]);
        }
    }
    HttpRequest {
        method: HttpMethod::Get,
        url: feed_url.to_string(),
        headers,
        body: None,
        body_base64: None,
    }
}

/// Interpret an [`HttpResult`] for the feed-refresh protocol.
///
/// The shape mirrors `FeedClient.FeedFetchResult` in the legacy Swift:
///   * `304` → [`FeedResult::NotModified`] carrying `cache` forward.
///   * `200` → parse the body, capture the new `ETag` / `Last-Modified`
///     from the response headers, return [`FeedResult::Parsed`].
///   * `HttpResult::Error` → [`FeedError::Transport`].
///   * Any other HTTP status → [`FeedError::Http`].
///
/// `feed_url` and `podcast_id` are passed through to the parser so the
/// returned `Podcast` retains the caller's identity (no re-derivation).
/// `now` is the caller's notion of wall time — the kernel injects it so
/// `EtagCache::last_refreshed` is deterministic against a test clock.
///
/// `prior_cache` carries the *previous* ETag/Last-Modified so a response with
/// omitted validator headers can preserve them. Pass `None` for the first
/// refresh.
pub fn handle_feed_response(
    feed_url: &Url,
    podcast_id: PodcastId,
    response: &HttpResult,
    prior_cache: Option<&EtagCache>,
    now: DateTime<Utc>,
) -> Result<FeedResult, FeedError> {
    match response {
        HttpResult::Error { message } => Err(FeedError::Transport(message.clone())),
        HttpResult::Ok {
            status_code,
            headers: _,
            body: _,
        } if *status_code == 304 => {
            let cache = response_cache(response, prior_cache, now);
            Ok(FeedResult::NotModified { cache })
        }
        HttpResult::Ok {
            status_code,
            headers: _,
            body: _,
        } if *status_code != 200 => Err(FeedError::Http {
            status_code: *status_code,
        }),
        HttpResult::Ok { headers, body, .. } => {
            let parsed = parse_feed(body.as_bytes(), feed_url, podcast_id)?;
            let cache = response_cache(response, prior_cache, now);
            // We deliberately replicated `headers` / `body` in the match arms
            // above to keep the borrow checker happy when we then call
            // `response.header(...)` here, which needs an immutable borrow of
            // the whole `response`.
            let _ = headers;
            let _ = body;
            Ok(FeedResult::Parsed {
                feed_url: feed_url.clone(),
                parsed,
                cache,
            })
        }
    }
}

fn response_cache(
    response: &HttpResult,
    prior: Option<&EtagCache>,
    now: DateTime<Utc>,
) -> EtagCache {
    let etag = response
        .header("ETag")
        .map(str::to_owned)
        .or_else(|| prior.and_then(|c| c.etag.clone()));
    let last_modified = response
        .header("Last-Modified")
        .map(str::to_owned)
        .or_else(|| prior.and_then(|c| c.last_modified.clone()));
    if etag.is_some() || last_modified.is_some() {
        EtagCache::with_headers(now, etag, last_modified)
    } else {
        EtagCache::new(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(status_code: u16, headers: Vec<Vec<String>>, body: &str) -> HttpResult {
        HttpResult::Ok {
            status_code,
            headers,
            body: body.to_owned(),
        }
    }

    #[test]
    fn not_modified_cache_prefers_response_validators() {
        let url = Url::parse("https://example.com/feed.xml").unwrap();
        let podcast_id = PodcastId::generate();
        let now = Utc::now();
        let prior = EtagCache::with_headers(
            now,
            Some("\"old\"".to_owned()),
            Some("Mon, 01 Jan 2024 00:00:00 GMT".to_owned()),
        );
        let response = ok(
            304,
            vec![
                vec!["ETag".to_owned(), "\"new\"".to_owned()],
                vec![
                    "Last-Modified".to_owned(),
                    "Tue, 02 Jan 2024 00:00:00 GMT".to_owned(),
                ],
            ],
            "",
        );

        let result = handle_feed_response(&url, podcast_id, &response, Some(&prior), now).unwrap();
        let FeedResult::NotModified { cache } = result else {
            panic!("expected NotModified");
        };

        assert_eq!(cache.etag.as_deref(), Some("\"new\""));
        assert_eq!(
            cache.last_modified.as_deref(),
            Some("Tue, 02 Jan 2024 00:00:00 GMT")
        );
    }

    #[test]
    fn not_modified_cache_carries_prior_validators_when_response_omits_them() {
        let url = Url::parse("https://example.com/feed.xml").unwrap();
        let podcast_id = PodcastId::generate();
        let now = Utc::now();
        let prior = EtagCache::with_headers(
            now,
            Some("\"old\"".to_owned()),
            Some("Mon, 01 Jan 2024 00:00:00 GMT".to_owned()),
        );
        let response = ok(304, vec![], "");

        let result = handle_feed_response(&url, podcast_id, &response, Some(&prior), now).unwrap();
        let FeedResult::NotModified { cache } = result else {
            panic!("expected NotModified");
        };

        assert_eq!(cache.etag.as_deref(), Some("\"old\""));
        assert_eq!(
            cache.last_modified.as_deref(),
            Some("Mon, 01 Jan 2024 00:00:00 GMT")
        );
    }
}
