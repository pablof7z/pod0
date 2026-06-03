/// Unit tests for `agent_note_handler` (feature #44).

use std::sync::{Arc, Mutex};
use crate::agent_note_handler::{build_agent_note_tags, handle_fetch_agent_notes, handle_publish_agent_note};
use crate::store::identity::IdentityStore;

const TEST_NSEC: &str = "nsec1cdxlq0ckkqeuauhzqaduugmrjpwuk3cgwq37ef2nvzje8at26lwqapk9us";
const TEST_PUBKEY_HEX: &str =
    "c7f5c9fc41894086a2fd8c3e542c1d6e6beeb2175ba41813de38bd02936bd4ff";
const PEER_PUBKEY_HEX: &str =
    "3bf0c63fcb93463407af97a5e5ee64fa883d107ef9e558472c4eb9aaaefa459d";

fn signed_in_identity() -> Arc<Mutex<IdentityStore>> {
    let mut id = IdentityStore::new();
    id.import_nsec(TEST_NSEC).unwrap();
    Arc::new(Mutex::new(id))
}

// ── publish: validation ───────────────────────────────────────────────

#[test]
fn publish_rejects_empty_content() {
    let identity = signed_in_identity();
    let v = handle_publish_agent_note(
        std::ptr::null_mut(), &identity, PEER_PUBKEY_HEX, "   ", None,
    );
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"], "empty note");
}

#[test]
fn publish_rejects_when_no_identity() {
    let identity = Arc::new(Mutex::new(IdentityStore::new()));
    let v = handle_publish_agent_note(
        std::ptr::null_mut(), &identity, PEER_PUBKEY_HEX, "hello", None,
    );
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"], "not signed in");
}

#[test]
fn publish_rejects_bad_recipient() {
    let identity = signed_in_identity();
    let v = handle_publish_agent_note(
        std::ptr::null_mut(), &identity, "not-a-pubkey", "hello", None,
    );
    assert_eq!(v["ok"], false);
    assert!(v["error"].as_str().unwrap().contains("recipient"));
}

/// Null app → signed (NMP publish skipped); event was valid.
#[test]
fn publish_returns_signed_with_null_app() {
    let identity = signed_in_identity();
    let v = handle_publish_agent_note(
        std::ptr::null_mut(), &identity, PEER_PUBKEY_HEX, "check this ep", None,
    );
    assert_eq!(v["ok"], true);
    assert_eq!(v["status"], "signed");
}

// ── tag building ──────────────────────────────────────────────────────

#[test]
fn tags_include_p_tag_for_recipient() {
    let tags = build_agent_note_tags(PEER_PUBKEY_HEX, None).unwrap();
    let has_p = tags.iter().any(|t| t.first().map(|s| s == "p").unwrap_or(false)
        && t.get(1).map(|s| s == PEER_PUBKEY_HEX).unwrap_or(false));
    assert!(has_p, "missing p tag");
    let has_e = tags.iter().any(|t| t.first().map(|s| s == "e").unwrap_or(false));
    assert!(!has_e, "thread-opener must not have e tag");
}

#[test]
fn reply_tags_carry_nip10_root_marker() {
    let root = "abc123abc123abc123abc123abc123abc123abc123abc123abc123abc123abcd";
    let tags = build_agent_note_tags(PEER_PUBKEY_HEX, Some(root)).unwrap();
    let e_tag = tags.iter().find(|t| t.first().map(|s| s == "e").unwrap_or(false));
    let e_tag = e_tag.expect("reply must have e tag");
    assert_eq!(e_tag.get(1).map(|s| s.as_str()), Some(root));
    assert_eq!(e_tag.get(3).map(|s| s.as_str()), Some("root"));
}

#[test]
fn empty_root_produces_no_e_tag() {
    let tags = build_agent_note_tags(PEER_PUBKEY_HEX, Some("")).unwrap();
    let has_e = tags.iter().any(|t| t.first().map(|s| s == "e").unwrap_or(false));
    assert!(!has_e);
}

// ── fetch: validation ─────────────────────────────────────────────────

#[test]
fn fetch_rejects_when_no_identity() {
    let identity = Arc::new(Mutex::new(IdentityStore::new()));
    let v = handle_fetch_agent_notes(std::ptr::null_mut(), &identity);
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"], "not signed in");
}

#[test]
fn fetch_with_null_app_returns_subscribed() {
    // With null app, push_interest_via_nmp is a no-op, but the action still
    // succeeds (subscription is fire-and-forget from the app's perspective).
    let identity = signed_in_identity();
    let v = handle_fetch_agent_notes(std::ptr::null_mut(), &identity);
    // null app → push_interest_via_nmp returns early, but we still say ok
    assert_eq!(v["ok"], true);
}
