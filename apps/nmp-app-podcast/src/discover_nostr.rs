//! `podcast.discover_nostr` host-op handler — NIP-F4 (`kind:10154`)
//! podcast discovery via a Nostr relay HTTP gateway.
//!
//! Lives in its own module so [`crate::host_op_handler::PodcastHostOpHandler`]
//! stays under the 500-line hard limit (AGENTS.md). The handler is a free
//! function that takes a [`HttpRequest`] dispatcher and the shared
//! `nostr_results` Arc so testing can drive it without spinning up an
//! `NmpApp`.
//!
//! ## Wire shape
//!
//! The default relay HTTP gateway is `https://api.nostr.band`, which exposes
//! a `GET /v0/search` endpoint that returns `{"events": [<NostrEvent>...]}`.
//! The handler is tolerant of alternative shapes:
//!
//! * **events array at root** — `[<event>, <event>, ...]`
//! * **events under `"events"`** — `{"events": [<event>, ...]}`
//! * **single event** — `{"id": "...", "kind": 10154, ...}`
//!
//! Each event is parsed via [`podcast_discovery::parse_event_json`]; events
//! that don't decode (wrong kind, missing title, malformed JSON) are
//! silently dropped (D6 — errors as data).
//!
//! ## Doctrine
//!
//! * **D6 — errors as data.** Returns `{"ok": false, "error": "..."}` on
//!   transport failure; the iOS shell surfaces it as a toast.
//! * **D7 — capabilities execute, never decide.** The HTTP capability
//!   performs the GET; this handler chooses the URL and parses the body.
//! * **Lock discipline.** The `nostr_results` lock is taken only AFTER
//!   the HTTP capability dispatch returns — same pattern as
//!   `handle_search_itunes`.

use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};

use podcast_discovery::NipF4Show;
use serde_json::Value;

use podcast_feeds::http::{HttpRequest, HttpResult};

use crate::ffi::projections::NostrShowSummary;

/// Default Nostr relay HTTP gateway. The `api.nostr.band` `v0/search`
/// endpoint indexes NIP-01 events across the relay network and returns a
/// JSON `{"events": [...]}` payload.
pub const DEFAULT_RELAY_HTTP_GATEWAY: &str = "https://api.nostr.band";

/// Build the HTTP request for a NIP-F4 discovery sweep.
///
/// `relay_url_override` lets the caller scope the sweep to a specific
/// gateway; `None` selects [`DEFAULT_RELAY_HTTP_GATEWAY`].
///
/// `query` narrows the search (delegated to the gateway's full-text
/// indexer); `None` performs a kind-only sweep.
pub fn build_discover_request(query: Option<&str>, relay_url_override: Option<&str>) -> HttpRequest {
    let base = relay_url_override
        .unwrap_or(DEFAULT_RELAY_HTTP_GATEWAY)
        .trim_end_matches('/');
    let encoded = url_encode(query.unwrap_or(""));
    // The gateway accepts `kind` as a filter param. We cap with `limit`
    // so an empty query doesn't pull the relay's full kind:10154 index.
    let url = format!("{base}/v0/search?q={encoded}&kind=10154&limit=50");
    HttpRequest::get(url, [("Accept", "application/json")])
}

/// Parse a relay HTTP response body into [`NipF4Show`]s.
///
/// Returns an empty Vec on any decode failure (D6). Tolerant of the three
/// response shapes documented at module level.
pub fn parse_discover_response(body: &str) -> Vec<NipF4Show> {
    let Ok(root) = serde_json::from_str::<Value>(body) else {
        return Vec::new();
    };
    let events = match &root {
        Value::Array(arr) => arr.clone(),
        Value::Object(map) => {
            if let Some(Value::Array(arr)) = map.get("events") {
                arr.clone()
            } else if map.contains_key("id") && map.contains_key("kind") {
                vec![root.clone()]
            } else {
                return Vec::new();
            }
        }
        _ => return Vec::new(),
    };
    events
        .into_iter()
        .filter_map(|ev| {
            let json = serde_json::to_string(&ev).ok()?;
            podcast_discovery::parse_nip_f4_event_json(&json)
        })
        .collect()
}

/// Project a [`NipF4Show`] onto the FFI-wire [`NostrShowSummary`].
pub fn project_show(show: &NipF4Show) -> NostrShowSummary {
    NostrShowSummary {
        event_id: show.event_id.clone(),
        author_pubkey: show.author_pubkey.clone(),
        title: show.title.clone(),
        description: show.description.clone(),
        feed_url: show.feed_url.clone(),
        artwork_url: show.artwork_url.clone(),
        categories: show.categories.clone(),
    }
}

/// Store the projected results into the shared snapshot slot and bump
/// `rev` so the next snapshot tick reflects them.
///
/// Returns the count stored.
pub fn write_results(
    results: Vec<NostrShowSummary>,
    slot: &Arc<Mutex<Vec<NostrShowSummary>>>,
    rev: &Arc<AtomicU64>,
) -> Result<usize, String> {
    let mut guard = slot
        .lock()
        .map_err(|_| "nostr_results poisoned".to_string())?;
    *guard = results;
    let count = guard.len();
    rev.fetch_add(1, Ordering::Relaxed);
    Ok(count)
}

pub fn handle_discover_nostr(
    query: Option<String>,
    relay_url: Option<String>,
    slot: &Arc<Mutex<Vec<NostrShowSummary>>>,
    rev: &Arc<AtomicU64>,
    fetch: impl FnOnce(&HttpRequest) -> Result<HttpResult, String>,
) -> Value {
    let req = build_discover_request(query.as_deref(), relay_url.as_deref());
    let http_result = match fetch(&req) {
        Ok(result) => result,
        Err(e) => return serde_json::json!({"ok": false, "error": e}),
    };
    let body = match &http_result {
        HttpResult::Ok { body, .. } => body.as_str(),
        HttpResult::Error { .. } => return error_envelope(&http_result),
    };
    let projected = parse_discover_response(body)
        .iter()
        .map(project_show)
        .collect();
    match write_results(projected, slot, rev) {
        Ok(count) => success_envelope(count),
        Err(e) => serde_json::json!({"ok": false, "error": e}),
    }
}

/// Build a `{"ok": true, ...}` envelope from a successful run.
pub fn success_envelope(count: usize) -> Value {
    serde_json::json!({"ok": true, "count": count})
}

/// Build a `{"ok": false, "error": ...}` envelope from a transport-layer
/// `HttpResult::Error`.
pub fn error_envelope(result: &HttpResult) -> Value {
    match result {
        HttpResult::Error { message } => serde_json::json!({"ok": false, "error": message}),
        HttpResult::Ok { .. } => serde_json::json!({"ok": false, "error": "unexpected ok in error_envelope"}),
    }
}

/// Percent-encode a query string for use in a URL parameter value.
/// Local copy so this module doesn't reach into `host_op_handler`.
fn url_encode(s: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_request_uses_default_gateway() {
        let req = build_discover_request(Some("rust"), None);
        assert!(req.url.starts_with("https://api.nostr.band"));
        assert!(req.url.contains("q=rust"));
        assert!(req.url.contains("kind=10154"));
    }

    #[test]
    fn build_request_honors_relay_override() {
        let req = build_discover_request(Some("ai"), Some("https://relay.example.com/"));
        // Trailing slash trimmed so we don't emit `//`.
        assert!(req.url.starts_with("https://relay.example.com/v0/search"));
        assert!(!req.url.contains("//v0"));
    }

    #[test]
    fn build_request_handles_empty_query() {
        let req = build_discover_request(None, None);
        assert!(req.url.contains("q=&kind=10154"));
    }

    #[test]
    fn build_request_percent_encodes_query() {
        let req = build_discover_request(Some("hello world"), None);
        assert!(req.url.contains("q=hello+world"));
    }

    #[test]
    fn parse_response_with_events_array_wrapper() {
        let body = r#"{
            "events": [
                {"id":"a","pubkey":"pk1","kind":10154,"created_at":0,"content":"","tags":[["title","Show A"]]},
                {"id":"b","pubkey":"pk2","kind":10154,"created_at":0,"content":"","tags":[["title","Show B"],["feed","https://feeds.example.com/b.rss"]]}
            ]
        }"#;
        let shows = parse_discover_response(body);
        assert_eq!(shows.len(), 2);
        assert_eq!(shows[0].title, "Show A");
        assert_eq!(shows[1].title, "Show B");
        assert_eq!(shows[1].feed_url.as_deref(), Some("https://feeds.example.com/b.rss"));
    }

    #[test]
    fn parse_response_with_top_level_array() {
        let body = r#"[
            {"id":"a","pubkey":"pk","kind":10154,"created_at":0,"content":"","tags":[["title","X"]]}
        ]"#;
        let shows = parse_discover_response(body);
        assert_eq!(shows.len(), 1);
        assert_eq!(shows[0].title, "X");
    }

    #[test]
    fn parse_response_with_single_event_object() {
        let body = r#"{"id":"a","pubkey":"pk","kind":10154,"created_at":0,"content":"","tags":[["title","Solo"]]}"#;
        let shows = parse_discover_response(body);
        assert_eq!(shows.len(), 1);
        assert_eq!(shows[0].title, "Solo");
    }

    #[test]
    fn parse_response_drops_wrong_kind_events() {
        let body = r#"{
            "events": [
                {"id":"a","pubkey":"pk","kind":1,"created_at":0,"content":"","tags":[["title","Note"]]},
                {"id":"b","pubkey":"pk","kind":10154,"created_at":0,"content":"","tags":[["title","Show"]]}
            ]
        }"#;
        let shows = parse_discover_response(body);
        assert_eq!(shows.len(), 1);
        assert_eq!(shows[0].title, "Show");
    }

    #[test]
    fn parse_response_drops_events_with_no_title() {
        let body = r#"{
            "events": [
                {"id":"a","pubkey":"pk","kind":10154,"created_at":0,"content":"","tags":[]}
            ]
        }"#;
        assert!(parse_discover_response(body).is_empty());
    }

    #[test]
    fn parse_response_returns_empty_on_garbage() {
        assert!(parse_discover_response("not json").is_empty());
        assert!(parse_discover_response("null").is_empty());
        assert!(parse_discover_response(r#""string""#).is_empty());
    }

    #[test]
    fn project_show_preserves_every_field() {
        let show = NipF4Show {
            event_id: "ev".into(),
            author_pubkey: "pk".into(),
            title: "T".into(),
            description: Some("D".into()),
            feed_url: Some("https://x.example/rss".into()),
            artwork_url: Some("https://img.example/c.jpg".into()),
            categories: vec!["Tech".into()],
        };
        let projected = project_show(&show);
        assert_eq!(projected.event_id, "ev");
        assert_eq!(projected.author_pubkey, "pk");
        assert_eq!(projected.title, "T");
        assert_eq!(projected.description.as_deref(), Some("D"));
        assert_eq!(projected.feed_url.as_deref(), Some("https://x.example/rss"));
        assert_eq!(projected.artwork_url.as_deref(), Some("https://img.example/c.jpg"));
        assert_eq!(projected.categories, vec!["Tech".to_string()]);
    }

    #[test]
    fn write_results_bumps_rev_and_replaces_slot() {
        let slot = Arc::new(Mutex::new(Vec::<NostrShowSummary>::new()));
        let rev = Arc::new(AtomicU64::new(7));
        let count = write_results(
            vec![NostrShowSummary {
                event_id: "ev".into(),
                author_pubkey: "pk".into(),
                title: "T".into(),
                ..Default::default()
            }],
            &slot,
            &rev,
        )
        .expect("ok");
        assert_eq!(count, 1);
        assert_eq!(rev.load(Ordering::Relaxed), 8);
        assert_eq!(slot.lock().unwrap().len(), 1);

        // Re-writing replaces (doesn't append).
        write_results(vec![], &slot, &rev).expect("ok");
        assert!(slot.lock().unwrap().is_empty());
        assert_eq!(rev.load(Ordering::Relaxed), 9);
    }

    #[test]
    fn success_envelope_contains_count() {
        let json = success_envelope(3);
        assert_eq!(json["ok"], true);
        assert_eq!(json["count"], 3);
    }

    #[test]
    fn error_envelope_surfaces_transport_message() {
        let result = HttpResult::Error { message: "DNS failure".into() };
        let json = error_envelope(&result);
        assert_eq!(json["ok"], false);
        assert_eq!(json["error"], "DNS failure");
    }
}
