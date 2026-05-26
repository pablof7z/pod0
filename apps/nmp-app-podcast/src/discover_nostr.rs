//! `podcast.discover_nostr` host-op handler — NIP-F4 (`kind:10154`)
//! podcast discovery via a Nostr relay subscription, with an HTTP gateway
//! fallback to `api.nostr.band`.
//!
//! Lives in its own module so [`crate::host_op_handler::PodcastHostOpHandler`]
//! stays under the 500-line hard limit (AGENTS.md). The handler is a free
//! function that takes a relay-dispatch closure and an HTTP-dispatch closure
//! so testing can drive it without spinning up an `NmpApp`.
//!
//! ## Primary path — WebSocket relay subscription
//!
//! Dispatches a `NostrRelayRequest::Subscribe` to `wss://relay.primal.net`
//! with filter `{"kinds":[10154],"limit":50}` (NIP-50 `"search"` field is
//! added when a query is provided). Events received before EOSE are parsed
//! via [`podcast_discovery::parse_nip_f4_event_json`].
//!
//! ## Fallback path — HTTP gateway
//!
//! If the relay path fails (transport error, decode error, or zero events
//! returned with EOSE), the handler tries the HTTP gateway (`api.nostr.band`
//! by default). This preserves behaviour when the relay is offline or the
//! capability executor is not yet wired in production.
//!
//! ## Wire shape (HTTP gateway)
//!
//! `GET /v0/search` returns `{"events": [<NostrEvent>...]}`. Tolerant of:
//!
//! * **events array at root** — `[<event>, <event>, ...]`
//! * **events under `"events"`** — `{"events": [<event>, ...]}`
//! * **single event** — `{"id": "...", "kind": 10154, ...}`
//!
//! Each event is parsed via [`podcast_discovery::parse_nip_f4_event_json`];
//! events that don't decode are silently dropped (D6 — errors as data).
//!
//! ## Doctrine
//!
//! * **D6 — errors as data.** Returns `{"ok": false, "error": "..."}` on
//!   transport failure; the iOS shell surfaces it as a toast.
//! * **D7 — capabilities execute, never decide.** The relay and HTTP
//!   capabilities perform the I/O; this handler chooses parameters and
//!   parses results.
//! * **Lock discipline.** The `nostr_results` lock is taken only AFTER
//!   the capability dispatch returns — same pattern as `handle_search_itunes`.

use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};

use podcast_discovery::NipF4Show;
use serde_json::Value;

use podcast_feeds::http::{HttpRequest, HttpResult};

use crate::capability::nostr_relay::{NostrRelayRequest, NostrRelayResult};
use crate::ffi::projections::NostrShowSummary;

/// Default relay for WebSocket subscription (primary path).
pub const DEFAULT_RELAY_WSS: &str = "wss://relay.primal.net";

/// Default Nostr relay HTTP gateway. The `api.nostr.band` `v0/search`
/// endpoint indexes NIP-01 events across the relay network and returns a
/// JSON `{"events": [...]}` payload.
pub const DEFAULT_RELAY_HTTP_GATEWAY: &str = "https://api.nostr.band";

/// Build the `NostrRelayRequest::Subscribe` for a NIP-F4 discovery sweep.
///
/// `relay_wss_override` lets the caller scope the sweep to a specific WSS
/// relay; `None` selects [`DEFAULT_RELAY_WSS`]. When `query` is `Some`
/// and non-empty the NIP-50 `"search"` key is added to the filter.
pub fn build_relay_request(
    query: Option<&str>,
    relay_wss_override: Option<&str>,
) -> NostrRelayRequest {
    let relay_url = relay_wss_override
        .unwrap_or(DEFAULT_RELAY_WSS)
        .to_string();
    let mut filter = serde_json::json!({"kinds": [10154], "limit": 50});
    if let Some(q) = query {
        if !q.is_empty() {
            filter["search"] = Value::String(q.to_string());
        }
    }
    NostrRelayRequest::Subscribe {
        sub_id: uuid::Uuid::new_v4().to_string(),
        filter,
        relay_urls: vec![relay_url],
        timeout_ms: 8000,
    }
}

/// Build the HTTP request for a NIP-F4 discovery sweep (fallback path).
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

/// Parse relay WebSocket events (from `NostrRelayResult::Events`) into
/// [`NipF4Show`]s. Each event value is serialized back to JSON and parsed
/// via [`podcast_discovery::parse_nip_f4_event_json`].
pub fn parse_relay_events(events: &[Value]) -> Vec<NipF4Show> {
    events
        .iter()
        .filter_map(|ev| {
            let json = serde_json::to_string(ev).ok()?;
            podcast_discovery::parse_nip_f4_event_json(&json)
        })
        .collect()
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

/// Handle a `podcast.discover_nostr` action.
///
/// Tries the relay subscription path first; falls back to the HTTP gateway
/// if the relay returns an error, fails to decode, or returns zero events
/// with EOSE (relay may lack kind:10154 index).
///
/// `relay_url` is used as a WSS relay when it starts with `wss://` or
/// `ws://`, or as an HTTP gateway override when it starts with `http`.
/// `None` uses the defaults for both paths.
pub fn handle_discover_nostr(
    query: Option<String>,
    relay_url: Option<String>,
    slot: &Arc<Mutex<Vec<NostrShowSummary>>>,
    rev: &Arc<AtomicU64>,
    relay_fetch: impl FnOnce(&NostrRelayRequest) -> Result<NostrRelayResult, String>,
    http_fetch: impl FnOnce(&HttpRequest) -> Result<HttpResult, String>,
) -> Value {
    // Determine WSS and HTTP overrides from relay_url based on scheme.
    let wss_override = relay_url.as_deref().and_then(|u| {
        if u.starts_with("wss://") || u.starts_with("ws://") {
            Some(u)
        } else {
            None
        }
    });
    let http_gateway_override = relay_url.as_deref().and_then(|u| {
        if u.starts_with("http://") || u.starts_with("https://") {
            Some(u)
        } else {
            None
        }
    });

    // --- Primary path: relay subscription ---
    let relay_req = build_relay_request(query.as_deref(), wss_override);
    let relay_shows: Option<Vec<NipF4Show>> = match relay_fetch(&relay_req) {
        Ok(NostrRelayResult::Events { events, eose: _ }) => {
            let shows = parse_relay_events(&events);
            if shows.is_empty() {
                // No parseable kind:10154 events (EOSE with empty list, or
                // timeout with no events) — fall back to HTTP gateway.
                None
            } else {
                Some(shows)
            }
        }
        // Transport error, decode failure, unexpected variant — fall back.
        Ok(_) | Err(_) => None,
    };

    if let Some(shows) = relay_shows {
        let projected = shows.iter().map(project_show).collect();
        return match write_results(projected, slot, rev) {
            Ok(count) => success_envelope(count),
            Err(e) => serde_json::json!({"ok": false, "error": e}),
        };
    }

    // --- Fallback path: HTTP gateway ---
    let req = build_discover_request(query.as_deref(), http_gateway_override);
    let http_result = match http_fetch(&req) {
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
#[path = "discover_nostr_tests.rs"]
mod tests;
