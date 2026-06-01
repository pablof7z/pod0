//! Unit tests for `social_publish_handler` — kind:0 / kind:1 / kind:9802
//! signing on behalf of the user's Nostr identity.
//!
//! The live relay round-trip (sign → publish) is integration-tested in the
//! headless scenario binary (`scenarios/social_publish.rs`). These unit
//! tests cover the validation / short-circuit paths and the pure
//! event-build helpers that don't need a live relay (the `app.is_null()`
//! guard returns `{"status":"signed"}` so the build + sign path still runs).

use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use nostr::Keys;

use crate::social_publish_handler::{
    build_highlight_event, build_note_event, build_profile_event, handle_publish_highlight,
    handle_publish_note, handle_publish_profile,
};
use crate::store::identity::IdentityStore;

const TEST_NSEC: &str = "nsec1cdxlq0ckkqeuauhzqaduugmrjpwuk3cgwq37ef2nvzje8at26lwqapk9us";
const TEST_PUBKEY_HEX: &str =
    "c7f5c9fc41894086a2fd8c3e542c1d6e6beeb2175ba41813de38bd02936bd4ff";

fn signed_in_identity() -> Arc<Mutex<IdentityStore>> {
    let mut id = IdentityStore::new();
    id.import_nsec(TEST_NSEC).unwrap();
    Arc::new(Mutex::new(id))
}

fn keys() -> Keys {
    Keys::parse(TEST_NSEC).unwrap()
}

// ── build_profile_event: pure kind:0 builder ─────────────────────────

#[test]
fn build_profile_event_is_kind_0_with_json_content() {
    let event = build_profile_event(
        &keys(),
        "alice",
        Some("Alice"),
        Some("about me"),
        Some("https://example.com/a.png"),
    )
    .unwrap();
    assert_eq!(event.kind, nostr::Kind::Metadata);
    // Content must be valid JSON carrying the supplied fields.
    let parsed: serde_json::Value = serde_json::from_str(&event.content).unwrap();
    assert_eq!(parsed["name"], "alice");
    assert_eq!(parsed["display_name"], "Alice");
    assert_eq!(parsed["about"], "about me");
    assert_eq!(parsed["picture"], "https://example.com/a.png");
}

#[test]
fn build_profile_event_omits_absent_optional_fields() {
    let event = build_profile_event(&keys(), "bob", None, None, None).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&event.content).unwrap();
    assert_eq!(parsed["name"], "bob");
    assert!(parsed.get("display_name").is_none());
    assert!(parsed.get("about").is_none());
    assert!(parsed.get("picture").is_none());
}

// ── build_note_event: pure kind:1 builder ────────────────────────────

#[test]
fn build_note_event_is_kind_1_with_content() {
    let event = build_note_event(&keys(), "hello world", None).unwrap();
    assert_eq!(event.kind, nostr::Kind::TextNote);
    assert_eq!(event.content, "hello world");
}

#[test]
fn build_note_event_carries_supplied_tags() {
    let tags = vec![
        vec!["t".to_string(), "note".to_string()],
        vec!["a".to_string(), "30311:abc:def".to_string()],
    ];
    let event = build_note_event(&keys(), "tagged", Some(&tags)).unwrap();
    let serialized: Vec<Vec<String>> = event
        .tags
        .iter()
        .map(|t| t.clone().to_vec())
        .collect();
    assert!(serialized.contains(&vec!["t".to_string(), "note".to_string()]));
    assert!(serialized.contains(&vec!["a".to_string(), "30311:abc:def".to_string()]));
}

// ── build_highlight_event: pure kind:9802 builder ────────────────────

#[test]
fn build_highlight_event_is_kind_9802() {
    let event = build_highlight_event(&keys(), "highlighted text", None).unwrap();
    assert_eq!(event.kind, nostr::Kind::Custom(9802));
    assert_eq!(event.content, "highlighted text");
}

#[test]
fn build_highlight_event_carries_full_nip73_tag_set() {
    // The full tag set Swift's `publishUserClip` assembles: enclosure `r`,
    // feed `r`, episode `i` coordinate, context, alt.
    let tags = vec![
        vec!["r".to_string(), "https://example.com/audio.mp3".to_string()],
        vec!["r".to_string(), "https://example.com/feed.xml".to_string()],
        vec![
            "i".to_string(),
            "podcast:item:guid:GUID#t=10,20".to_string(),
        ],
        vec!["context".to_string(), "surrounding context".to_string()],
        vec!["alt".to_string(), "a caption".to_string()],
    ];
    let event = build_highlight_event(&keys(), "quote", Some(&tags)).unwrap();
    let serialized: Vec<Vec<String>> = event.tags.iter().map(|t| t.clone().to_vec()).collect();
    assert!(serialized.contains(&vec!["context".to_string(), "surrounding context".to_string()]));
    assert!(serialized.contains(&vec!["alt".to_string(), "a caption".to_string()]));
    assert!(serialized
        .contains(&vec!["r".to_string(), "https://example.com/feed.xml".to_string()]));
    assert!(serialized.contains(&vec![
        "i".to_string(),
        "podcast:item:guid:GUID#t=10,20".to_string()
    ]));
}

// ── handle_*: validation / short-circuit ─────────────────────────────

#[test]
fn publish_profile_rejects_when_no_identity() {
    let identity = Arc::new(Mutex::new(IdentityStore::new())); // no key
    let v = handle_publish_profile(
        std::ptr::null_mut(),
        &identity,
        "alice",
        None,
        None,
        None,
        "corr",
    );
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"], "not signed in");
}

#[test]
fn publish_profile_signs_under_null_app() {
    let identity = signed_in_identity();
    let v = handle_publish_profile(
        std::ptr::null_mut(),
        &identity,
        "alice",
        Some("Alice"),
        None,
        None,
        "corr",
    );
    assert_eq!(v["ok"], true);
    assert_eq!(v["status"], "signed");
    assert!(v["event_id"].as_str().is_some());
}

#[test]
fn publish_note_rejects_empty_content() {
    let identity = signed_in_identity();
    let v = handle_publish_note(std::ptr::null_mut(), &identity, "   ", None, "corr");
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"], "empty note");
}

#[test]
fn publish_note_rejects_when_no_identity() {
    let identity = Arc::new(Mutex::new(IdentityStore::new()));
    let v = handle_publish_note(std::ptr::null_mut(), &identity, "hi", None, "corr");
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"], "not signed in");
}

#[test]
fn publish_note_signs_under_null_app() {
    let identity = signed_in_identity();
    let v = handle_publish_note(std::ptr::null_mut(), &identity, "hello", None, "corr");
    assert_eq!(v["ok"], true);
    assert_eq!(v["status"], "signed");
    let _ = TEST_PUBKEY_HEX; // keep referenced; pubkey assertions live in scenario.
}

#[test]
fn publish_highlight_rejects_empty_content() {
    let identity = signed_in_identity();
    let v = handle_publish_highlight(std::ptr::null_mut(), &identity, "  ", None, "corr");
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"], "empty highlight");
}

#[test]
fn publish_highlight_signs_under_null_app() {
    let identity = signed_in_identity();
    let tags = vec![vec!["context".to_string(), "ctx".to_string()]];
    let v = handle_publish_highlight(
        std::ptr::null_mut(),
        &identity,
        "a quote",
        Some(&tags),
        "corr",
    );
    assert_eq!(v["ok"], true);
    assert_eq!(v["status"], "signed");
    assert!(v["event_id"].as_str().is_some());
}
