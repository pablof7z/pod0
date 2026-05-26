//! Tests for [super::nip_f4] — NIP-F4 show event parsing (kind 10154).
//!
//! Extracted from `nip_f4.rs` to keep that file under the 500-line hard limit.

use super::*;

fn full_tags() -> Vec<Vec<String>> {
    vec![
        vec!["title".into(), "Rust Talk".into()],
        vec!["summary".into(), "A show about Rust".into()],
        vec!["image".into(), "https://img.example/cover.jpg".into()],
        vec!["feed".into(), "https://feeds.example.com/rust.rss".into()],
        vec!["category".into(), "Technology".into()],
        vec!["category".into(), "Programming".into()],
    ]
}

#[test]
fn kind_constants_pinned() {
    assert_eq!(KIND_NIP_F4_SHOW, 10154);
    assert_eq!(KIND_NIP_F4_EPISODE, 54);
    assert_eq!(KIND_NIP_F4_AUTHOR_CLAIM, 10064);
}

#[test]
fn parse_full_show_collects_every_field() {
    let show = parse_kind_10154(
        KIND_NIP_F4_SHOW,
        "ev-id",
        "podcast-pk",
        "",
        &full_tags(),
    )
    .expect("parse");
    assert_eq!(show.event_id, "ev-id");
    assert_eq!(show.author_pubkey, "podcast-pk");
    assert_eq!(show.title, "Rust Talk");
    assert_eq!(show.description.as_deref(), Some("A show about Rust"));
    assert_eq!(show.artwork_url.as_deref(), Some("https://img.example/cover.jpg"));
    assert_eq!(
        show.feed_url.as_deref(),
        Some("https://feeds.example.com/rust.rss")
    );
    assert_eq!(
        show.categories,
        vec!["Technology".to_string(), "Programming".into()]
    );
}

#[test]
fn parse_minimal_show_with_only_title_succeeds() {
    let tags = vec![vec!["title".into(), "Solo".into()]];
    let show = parse_kind_10154(KIND_NIP_F4_SHOW, "id", "pk", "", &tags).expect("parse");
    assert_eq!(show.title, "Solo");
    assert!(show.description.is_none());
    assert!(show.feed_url.is_none());
    assert!(show.artwork_url.is_none());
    assert!(show.categories.is_empty());
}

#[test]
fn parse_rejects_wrong_kind() {
    let err = parse_kind_10154(30074, "id", "pk", "", &full_tags()).unwrap_err();
    assert!(matches!(
        err,
        ParseError::WrongKind {
            expected: KIND_NIP_F4_SHOW,
            got: 30074
        }
    ));
}

#[test]
fn parse_requires_a_title_or_content() {
    // No title tag, empty content.
    let tags = vec![vec!["feed".into(), "https://x.example/rss".into()]];
    let err = parse_kind_10154(KIND_NIP_F4_SHOW, "id", "pk", "", &tags).unwrap_err();
    assert_eq!(err, ParseError::MissingTag("title"));
}

#[test]
fn parse_falls_back_title_to_content_prefix() {
    let tags = vec![vec!["feed".into(), "https://x.example/rss".into()]];
    let show = parse_kind_10154(
        KIND_NIP_F4_SHOW,
        "id",
        "pk",
        "Content-as-title fallback",
        &tags,
    )
    .expect("parse");
    assert_eq!(show.title, "Content-as-title fallback");
    // Description falls back to content too.
    assert_eq!(show.description.as_deref(), Some("Content-as-title fallback"));
}

#[test]
fn parse_rejects_empty_title_tag() {
    // Title tag present but value is empty — first_tag_value drops it,
    // and with no content fallback the parse fails with MissingTag.
    let tags = vec![vec!["title".into(), String::new()]];
    let err = parse_kind_10154(KIND_NIP_F4_SHOW, "id", "pk", "", &tags).unwrap_err();
    assert_eq!(err, ParseError::MissingTag("title"));
}

#[test]
fn parse_ignores_unknown_tags() {
    let tags = vec![
        vec!["title".into(), "Show".into()],
        vec!["foreign".into(), "value".into()],
        vec!["e".into(), "ref-id".into()],
    ];
    let show = parse_kind_10154(KIND_NIP_F4_SHOW, "id", "pk", "", &tags).expect("parse");
    assert_eq!(show.title, "Show");
    assert!(show.categories.is_empty());
}

// ── parse_event_json ──────────────────────────────────────────────────

#[test]
fn parse_event_json_handles_full_event() {
    let json = r#"{
        "id": "abc123",
        "pubkey": "deadbeef",
        "kind": 10154,
        "created_at": 1700000000,
        "content": "show notes",
        "tags": [
            ["title", "Test"],
            ["feed", "https://feeds.example.com/x.rss"]
        ]
    }"#;
    let show = parse_event_json(json).expect("decode");
    assert_eq!(show.event_id, "abc123");
    assert_eq!(show.author_pubkey, "deadbeef");
    assert_eq!(show.title, "Test");
    assert_eq!(
        show.feed_url.as_deref(),
        Some("https://feeds.example.com/x.rss")
    );
    // Content used as description fallback when no summary tag.
    assert_eq!(show.description.as_deref(), Some("show notes"));
}

#[test]
fn parse_event_json_drops_wrong_kind() {
    let json = r#"{
        "id": "id", "pubkey": "pk", "kind": 1,
        "tags": [["title","X"]], "content": ""
    }"#;
    assert!(parse_event_json(json).is_none());
}

#[test]
fn parse_event_json_drops_missing_title() {
    let json = r#"{
        "id": "id", "pubkey": "pk", "kind": 10154,
        "tags": [], "content": ""
    }"#;
    assert!(parse_event_json(json).is_none());
}

#[test]
fn parse_event_json_drops_garbage() {
    assert!(parse_event_json("not json").is_none());
    assert!(parse_event_json("{}").is_none());
    assert!(parse_event_json("[]").is_none());
}

#[test]
fn parse_event_json_ignores_unknown_envelope_fields() {
    // Forward-compat: relay wrappers may add metadata around the
    // event ("relays": [...], "score": 0.42, …). We only care about
    // the canonical NIP-01 fields.
    let json = r#"{
        "id": "id1", "pubkey": "pk1", "kind": 10154,
        "created_at": 0, "sig": "...",
        "extra": {"score": 0.42},
        "tags": [["title","Y"]], "content": ""
    }"#;
    let show = parse_event_json(json).expect("decode");
    assert_eq!(show.title, "Y");
}
