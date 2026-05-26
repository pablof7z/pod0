//! Tests for [`super::discover_nostr`] — request building, response parsing, and
//! result projection for NIP-F4 Nostr podcast discovery.
//!
//! Extracted from `discover_nostr.rs` to keep that file under the 500-line hard limit.

use crate::capability::nostr_relay::NostrRelayResult;

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

// --- Relay request building ---

#[test]
fn build_relay_request_uses_default_wss() {
    let req = build_relay_request(None, None);
    match req {
        NostrRelayRequest::Subscribe { relay_urls, filter, timeout_ms, .. } => {
            assert_eq!(relay_urls, vec!["wss://relay.primal.net"]);
            assert_eq!(filter["kinds"][0], 10154);
            assert_eq!(filter["limit"], 50);
            assert!(filter.get("search").is_none(), "no search key when query is None");
            assert_eq!(timeout_ms, 8000);
        }
        _ => panic!("expected Subscribe variant"),
    }
}

#[test]
fn build_relay_request_adds_search_when_query_given() {
    let req = build_relay_request(Some("rust"), None);
    match req {
        NostrRelayRequest::Subscribe { filter, .. } => {
            assert_eq!(filter["search"], "rust");
        }
        _ => panic!("expected Subscribe variant"),
    }
}

#[test]
fn build_relay_request_skips_search_for_empty_query() {
    let req = build_relay_request(Some(""), None);
    match req {
        NostrRelayRequest::Subscribe { filter, .. } => {
            assert!(filter.get("search").is_none(), "empty query must not add search key");
        }
        _ => panic!("expected Subscribe variant"),
    }
}

#[test]
fn build_relay_request_honors_wss_override() {
    let req = build_relay_request(None, Some("wss://relay.example.com"));
    match req {
        NostrRelayRequest::Subscribe { relay_urls, .. } => {
            assert_eq!(relay_urls, vec!["wss://relay.example.com"]);
        }
        _ => panic!("expected Subscribe variant"),
    }
}

// --- parse_relay_events ---

#[test]
fn parse_relay_events_decodes_kind_10154() {
    let events = vec![
        serde_json::json!({"id":"a","pubkey":"pk","kind":10154,"created_at":0,"content":"","tags":[["title","Relay Show"]]}),
    ];
    let shows = parse_relay_events(&events);
    assert_eq!(shows.len(), 1);
    assert_eq!(shows[0].title, "Relay Show");
}

#[test]
fn parse_relay_events_drops_wrong_kind() {
    let events = vec![
        serde_json::json!({"id":"a","pubkey":"pk","kind":1,"created_at":0,"content":"hello","tags":[]}),
    ];
    assert!(parse_relay_events(&events).is_empty());
}

// --- handle_discover_nostr: relay-first path ---

fn make_slot() -> Arc<Mutex<Vec<NostrShowSummary>>> {
    Arc::new(Mutex::new(Vec::new()))
}
fn make_rev() -> Arc<AtomicU64> {
    Arc::new(AtomicU64::new(0))
}

#[test]
fn handle_discover_nostr_uses_relay_result_when_events_found() {
    let slot = make_slot();
    let rev = make_rev();
    let relay_events = vec![
        serde_json::json!({"id":"a","pubkey":"pk","kind":10154,"created_at":0,"content":"","tags":[["title","Relay Show"]]}),
    ];
    let result = handle_discover_nostr(
        None,
        None,
        &slot,
        &rev,
        |_req| {
            Ok(NostrRelayResult::Events {
                events: relay_events.clone(),
                eose: true,
            })
        },
        |_req| panic!("http fallback must not be called when relay succeeds"),
    );
    assert_eq!(result["ok"], true);
    assert_eq!(result["count"], 1);
    assert_eq!(slot.lock().unwrap().len(), 1);
    assert_eq!(slot.lock().unwrap()[0].title, "Relay Show");
}

#[test]
fn handle_discover_nostr_falls_back_to_http_on_relay_error() {
    let slot = make_slot();
    let rev = make_rev();
    let http_body = r#"{"events":[{"id":"b","pubkey":"pk","kind":10154,"created_at":0,"content":"","tags":[["title","HTTP Show"]]}]}"#;
    let result = handle_discover_nostr(
        None,
        None,
        &slot,
        &rev,
        |_req| Ok(NostrRelayResult::Error { message: "timeout".into() }),
        |_req| {
            Ok(HttpResult::Ok {
                status_code: 200,
                headers: vec![],
                body: http_body.to_string(),
            })
        },
    );
    assert_eq!(result["ok"], true);
    assert_eq!(result["count"], 1);
    assert_eq!(slot.lock().unwrap()[0].title, "HTTP Show");
}

#[test]
fn handle_discover_nostr_falls_back_to_http_when_relay_eose_empty() {
    let slot = make_slot();
    let rev = make_rev();
    let http_body = r#"{"events":[{"id":"c","pubkey":"pk","kind":10154,"created_at":0,"content":"","tags":[["title","Fallback Show"]]}]}"#;
    let result = handle_discover_nostr(
        None,
        None,
        &slot,
        &rev,
        |_req| Ok(NostrRelayResult::Events { events: vec![], eose: true }),
        |_req| {
            Ok(HttpResult::Ok {
                status_code: 200,
                headers: vec![],
                body: http_body.to_string(),
            })
        },
    );
    assert_eq!(result["ok"], true);
    assert_eq!(result["count"], 1);
    assert_eq!(slot.lock().unwrap()[0].title, "Fallback Show");
}

#[test]
fn handle_discover_nostr_falls_back_to_http_when_relay_timeout_no_eose() {
    // Relay timed out (eose=false) with no events — should fall back to HTTP
    // because we have no relay data to surface.
    let slot = make_slot();
    let rev = make_rev();
    let http_body = r#"{"events":[{"id":"e","pubkey":"pk","kind":10154,"created_at":0,"content":"","tags":[["title","Timeout Fallback"]]}]}"#;
    let result = handle_discover_nostr(
        None,
        None,
        &slot,
        &rev,
        |_req| Ok(NostrRelayResult::Events { events: vec![], eose: false }),
        |_req| {
            Ok(HttpResult::Ok {
                status_code: 200,
                headers: vec![],
                body: http_body.to_string(),
            })
        },
    );
    assert_eq!(result["ok"], true);
    assert_eq!(result["count"], 1);
    assert_eq!(slot.lock().unwrap()[0].title, "Timeout Fallback");
}

#[test]
fn handle_discover_nostr_falls_back_to_http_on_relay_dispatch_error() {
    let slot = make_slot();
    let rev = make_rev();
    let http_body = r#"{"events":[{"id":"d","pubkey":"pk","kind":10154,"created_at":0,"content":"","tags":[["title","Err Fallback"]]}]}"#;
    let result = handle_discover_nostr(
        None,
        None,
        &slot,
        &rev,
        |_req| Err("capability not wired".into()),
        |_req| {
            Ok(HttpResult::Ok {
                status_code: 200,
                headers: vec![],
                body: http_body.to_string(),
            })
        },
    );
    assert_eq!(result["ok"], true);
    assert_eq!(result["count"], 1);
}
