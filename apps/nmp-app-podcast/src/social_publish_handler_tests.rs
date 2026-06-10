//! Unit tests for `social_publish_handler` — kind:0 / kind:1 / kind:9802
//! publishing on behalf of the user's Nostr identity.
//!
//! Signing moved to the kernel's active-account signer; these handlers no
//! longer build or sign `nostr::Event`s. The live relay round-trip
//! (kernel sign → publish) is integration-tested in the headless scenario
//! binary. These unit tests cover the validation / short-circuit paths and
//! the pure kind:0 `fields` assembly helper. Under a null `app` the dispatch
//! helpers short-circuit to `{"status":"signed"}` without touching the FFI.

use std::sync::{Arc, Mutex};

use crate::social_publish_handler::{
    build_highlight_tags, build_note_tags, build_profile_fields, handle_publish_highlight,
    handle_publish_note, handle_publish_profile, HighlightFields,
};
use crate::store::identity::IdentityStore;

const TEST_NSEC: &str = "nsec1cdxlq0ckkqeuauhzqaduugmrjpwuk3cgwq37ef2nvzje8at26lwqapk9us";

fn signed_in_identity() -> Arc<Mutex<IdentityStore>> {
    let mut id = IdentityStore::new();
    id.import_nsec(TEST_NSEC).unwrap();
    Arc::new(Mutex::new(id))
}

// ── build_profile_fields: pure kind:0 field assembly ─────────────────

#[test]
fn build_profile_fields_carries_all_present_fields() {
    let fields = build_profile_fields(
        "alice",
        Some("Alice"),
        Some("about me"),
        Some("https://example.com/a.png"),
    );
    assert_eq!(fields["name"], "alice");
    assert_eq!(fields["display_name"], "Alice");
    assert_eq!(fields["about"], "about me");
    assert_eq!(fields["picture"], "https://example.com/a.png");
}

#[test]
fn build_profile_fields_omits_absent_optional_fields() {
    let fields = build_profile_fields("bob", None, None, None);
    assert_eq!(fields["name"], "bob");
    assert!(!fields.contains_key("display_name"));
    assert!(!fields.contains_key("about"));
    assert!(!fields.contains_key("picture"));
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
fn publish_profile_dispatches_under_null_app() {
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
fn publish_note_dispatches_under_null_app() {
    let identity = signed_in_identity();
    let v = handle_publish_note(std::ptr::null_mut(), &identity, "hello", None, "corr");
    assert_eq!(v["ok"], true);
    assert_eq!(v["status"], "signed");
}

fn empty_highlight_fields() -> HighlightFields<'static> {
    HighlightFields {
        enclosure_url: None,
        feed_url: None,
        item_guid: None,
        start_sec: None,
        end_sec: None,
        caption: None,
    }
}

#[test]
fn publish_highlight_rejects_empty_content() {
    let identity = signed_in_identity();
    let v = handle_publish_highlight(
        std::ptr::null_mut(),
        &identity,
        "  ",
        &empty_highlight_fields(),
        "corr",
    );
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"], "empty highlight");
}

#[test]
fn publish_highlight_dispatches_under_null_app() {
    let identity = signed_in_identity();
    let v = handle_publish_highlight(
        std::ptr::null_mut(),
        &identity,
        "a quote",
        &empty_highlight_fields(),
        "corr",
    );
    assert_eq!(v["ok"], true);
    assert_eq!(v["status"], "signed");
}

// ── pure tag builders (the moved-from-Swift NIP-73/84 assembly) ───────

#[test]
fn build_note_tags_marks_note_and_omits_absent_coord() {
    assert_eq!(build_note_tags(None), vec![vec!["t".to_string(), "note".to_string()]]);
    assert_eq!(build_note_tags(Some("")), vec![vec!["t".to_string(), "note".to_string()]]);
}

#[test]
fn build_note_tags_prepends_episode_coord() {
    assert_eq!(
        build_note_tags(Some("30311:abc:def")),
        vec![
            vec!["a".to_string(), "30311:abc:def".to_string()],
            vec!["t".to_string(), "note".to_string()],
        ]
    );
}

#[test]
fn build_highlight_tags_assembles_full_nip73_84_set() {
    let f = HighlightFields {
        enclosure_url: Some("https://cdn.example.com/ep.mp3"),
        feed_url: Some("https://example.com/feed.xml"),
        item_guid: Some("GUID-1"),
        start_sec: Some(12),
        end_sec: Some(34),
        caption: Some("nice bit"),
    };
    assert_eq!(
        build_highlight_tags("the quote", &f),
        vec![
            vec!["r".to_string(), "https://cdn.example.com/ep.mp3".to_string()],
            vec!["r".to_string(), "https://example.com/feed.xml".to_string()],
            vec!["i".to_string(), "podcast:item:guid:GUID-1#t=12,34".to_string()],
            vec!["context".to_string(), "the quote".to_string()],
            vec!["alt".to_string(), "nice bit".to_string()],
        ]
    );
}

#[test]
fn build_highlight_tags_degrades_with_only_context() {
    // No episode/podcast resolved and no caption: just the context tag.
    assert_eq!(
        build_highlight_tags("ctx", &empty_highlight_fields()),
        vec![vec!["context".to_string(), "ctx".to_string()]]
    );
}

#[test]
fn build_highlight_tags_omits_empty_caption_and_defaults_times() {
    let f = HighlightFields {
        enclosure_url: None,
        feed_url: None,
        item_guid: Some("G"),
        start_sec: None,
        end_sec: None,
        caption: Some(""),
    };
    assert_eq!(
        build_highlight_tags("c", &f),
        vec![
            vec!["i".to_string(), "podcast:item:guid:G#t=0,0".to_string()],
            vec!["context".to_string(), "c".to_string()],
        ]
    );
}
