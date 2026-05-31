//! Unit tests for `agent_note_handler` (feature #44).
//!
//! The live relay round-trip (publish → subscribe → parse) is
//! integration-tested in the headless scenario binary
//! (`scenarios/agent_notes.rs`). These unit tests cover the
//! validation / short-circuit paths and the pure event-build / parse
//! helpers that don't need a live relay.

use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use nostr::Keys;

use crate::agent_note_handler::{
    build_agent_note_event, handle_fetch_agent_notes, handle_publish_agent_note, parse_agent_note,
    parse_inbound_notes,
};
use crate::ffi::projections::AgentNoteSummary;
use crate::store::identity::IdentityStore;

const TEST_NSEC: &str = "nsec1cdxlq0ckkqeuauhzqaduugmrjpwuk3cgwq37ef2nvzje8at26lwqapk9us";
const TEST_PUBKEY_HEX: &str =
    "c7f5c9fc41894086a2fd8c3e542c1d6e6beeb2175ba41813de38bd02936bd4ff";
// An arbitrary valid x-only pubkey distinct from TEST_PUBKEY_HEX, used as
// the note recipient.
const PEER_PUBKEY_HEX: &str =
    "3bf0c63fcb93463407af97a5e5ee64fa883d107ef9e558472c4eb9aaaefa459d";

fn signed_in_identity() -> Arc<Mutex<IdentityStore>> {
    let mut id = IdentityStore::new();
    id.import_nsec(TEST_NSEC).unwrap();
    Arc::new(Mutex::new(id))
}

// ── publish: validation / short-circuit ──────────────────────────────

#[test]
fn publish_rejects_empty_content() {
    let identity = signed_in_identity();
    let v = handle_publish_agent_note(
        std::ptr::null_mut(),
        &identity,
        PEER_PUBKEY_HEX,
        "   ",
        None,
        "corr",
    );
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"], "empty note");
}

#[test]
fn publish_rejects_when_no_identity() {
    let identity = Arc::new(Mutex::new(IdentityStore::new())); // no key
    let v = handle_publish_agent_note(
        std::ptr::null_mut(),
        &identity,
        PEER_PUBKEY_HEX,
        "hello peer",
        None,
        "corr",
    );
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"], "not signed in");
}

#[test]
fn publish_rejects_bad_recipient() {
    let identity = signed_in_identity();
    let v = handle_publish_agent_note(
        std::ptr::null_mut(),
        &identity,
        "not-a-pubkey",
        "hello peer",
        None,
        "corr",
    );
    assert_eq!(v["ok"], false);
    assert!(
        v["error"].as_str().unwrap().contains("recipient"),
        "expected recipient error, got {v}"
    );
}

/// With a null app pointer (unit-test mode) a valid note is signed but the
/// relay dispatch is skipped — status is `"signed"`, never `"published"`.
#[test]
fn publish_returns_signed_status_with_null_app() {
    let identity = signed_in_identity();
    let v = handle_publish_agent_note(
        std::ptr::null_mut(),
        &identity,
        PEER_PUBKEY_HEX,
        "recommendation: check out this episode",
        None,
        "corr",
    );
    assert_eq!(v["ok"], true);
    assert_eq!(v["status"], "signed");
    // Event id is a 64-char lowercase hex string.
    let id = v["event_id"].as_str().unwrap();
    assert_eq!(id.len(), 64);
    assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
}

// ── publish: event construction (pure helper) ────────────────────────

#[test]
fn build_event_is_kind1_with_recipient_p_tag() {
    let keys = Keys::parse(TEST_NSEC).unwrap();
    let event = build_agent_note_event(&keys, PEER_PUBKEY_HEX, "hi", None).unwrap();

    assert_eq!(event.kind, nostr::Kind::TextNote);
    assert_eq!(event.content, "hi");
    assert_eq!(event.pubkey.to_hex(), TEST_PUBKEY_HEX);

    // Re-serialise to inspect the wire tags exactly.
    let json: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
    let tags = json["tags"].as_array().unwrap();
    let has_p = tags.iter().any(|t| {
        let a = t.as_array().unwrap();
        a.first().and_then(|v| v.as_str()) == Some("p")
            && a.get(1).and_then(|v| v.as_str()) == Some(PEER_PUBKEY_HEX)
    });
    assert!(has_p, "missing recipient p tag in {tags:?}");
    // No reply → no e tag.
    let has_e = tags
        .iter()
        .any(|t| t.as_array().unwrap().first().and_then(|v| v.as_str()) == Some("e"));
    assert!(!has_e, "thread-opening note must not carry an e tag");
}

#[test]
fn build_reply_carries_nip10_root_marker() {
    let keys = Keys::parse(TEST_NSEC).unwrap();
    let root = "abc123abc123abc123abc123abc123abc123abc123abc123abc123abc123abcd";
    let event = build_agent_note_event(&keys, PEER_PUBKEY_HEX, "reply", Some(root)).unwrap();

    let json: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
    let tags = json["tags"].as_array().unwrap();
    // NIP-10 root marker: ["e", root, "", "root"].
    let root_tag = tags
        .iter()
        .find(|t| t.as_array().unwrap().first().and_then(|v| v.as_str()) == Some("e"))
        .expect("reply must carry an e tag");
    let parts = root_tag.as_array().unwrap();
    assert_eq!(parts[1].as_str(), Some(root));
    assert_eq!(parts[3].as_str(), Some("root"));
}

#[test]
fn build_empty_root_is_treated_as_no_reply() {
    let keys = Keys::parse(TEST_NSEC).unwrap();
    let event = build_agent_note_event(&keys, PEER_PUBKEY_HEX, "hi", Some("")).unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
    let has_e = json["tags"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t.as_array().unwrap().first().and_then(|v| v.as_str()) == Some("e"));
    assert!(!has_e, "empty root id must not produce an e tag");
}

// ── fetch: validation ────────────────────────────────────────────────

#[test]
fn fetch_rejects_when_no_identity() {
    let identity = Arc::new(Mutex::new(IdentityStore::new()));
    let cache: Arc<Mutex<Vec<AgentNoteSummary>>> = Arc::new(Mutex::new(Vec::new()));
    let rev = Arc::new(AtomicU64::new(0));
    let v = handle_fetch_agent_notes(std::ptr::null_mut(), &identity, &cache, &rev, "corr");
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"], "not signed in");
}

#[test]
fn fetch_rejects_with_null_app() {
    let identity = signed_in_identity();
    let cache: Arc<Mutex<Vec<AgentNoteSummary>>> = Arc::new(Mutex::new(Vec::new()));
    let rev = Arc::new(AtomicU64::new(0));
    let v = handle_fetch_agent_notes(std::ptr::null_mut(), &identity, &cache, &rev, "corr");
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"], "no app pointer");
    // rev must not advance on a short-circuit failure.
    assert_eq!(rev.load(std::sync::atomic::Ordering::Relaxed), 0);
}

// ── parse: inbound frame → AgentNoteSummary ──────────────────────────

#[test]
fn parse_note_extracts_fields_and_marks_untrusted() {
    let ev = serde_json::json!({
        "id": "deadbeef",
        "pubkey": PEER_PUBKEY_HEX,
        "content": "have you heard episode 42?",
        "created_at": 1_700_000_000_i64,
        "tags": [["p", TEST_PUBKEY_HEX]],
    });
    let note = parse_agent_note(&ev).unwrap();
    assert_eq!(note.id, "deadbeef");
    assert_eq!(note.content, "have you heard episode 42?");
    assert_eq!(note.created_at, 1_700_000_000);
    assert!(note.author_npub.starts_with("npub1"));
    assert_eq!(note.root_event_id, None);
    // Trust gate not implemented — every inbound note is untrusted.
    assert!(!note.trusted, "inbound notes must be untrusted until the trust gate lands");
}

#[test]
fn parse_note_without_id_is_dropped() {
    let ev = serde_json::json!({
        "pubkey": PEER_PUBKEY_HEX,
        "content": "no id here",
        "created_at": 1,
        "tags": [],
    });
    assert!(parse_agent_note(&ev).is_none());
}

#[test]
fn parse_note_extracts_nip10_root_marker() {
    let root = "1111111111111111111111111111111111111111111111111111111111111111";
    let ev = serde_json::json!({
        "id": "reply-id",
        "pubkey": PEER_PUBKEY_HEX,
        "content": "re: episode 42",
        "created_at": 2,
        "tags": [
            ["e", "2222222222222222222222222222222222222222222222222222222222222222"],
            ["e", root, "", "root"],
            ["p", TEST_PUBKEY_HEX],
        ],
    });
    let note = parse_agent_note(&ev).unwrap();
    // The marked "root" tag wins over the earlier positional e tag.
    assert_eq!(note.root_event_id.as_deref(), Some(root));
}

#[test]
fn parse_inbound_drops_self_authored_and_sorts_newest_first() {
    let events = vec![
        // Foreign, older.
        serde_json::json!({
            "id": "older-foreign",
            "pubkey": PEER_PUBKEY_HEX,
            "content": "older",
            "created_at": 100,
            "tags": [["p", TEST_PUBKEY_HEX]],
        }),
        // Self-authored — must be dropped even though it's tagged to us.
        serde_json::json!({
            "id": "self-note",
            "pubkey": TEST_PUBKEY_HEX,
            "content": "my own broadcast",
            "created_at": 999,
            "tags": [["p", TEST_PUBKEY_HEX]],
        }),
        // Foreign, newer.
        serde_json::json!({
            "id": "newer-foreign",
            "pubkey": PEER_PUBKEY_HEX,
            "content": "newer",
            "created_at": 200,
            "tags": [["p", TEST_PUBKEY_HEX]],
        }),
    ];
    let notes = parse_inbound_notes(&events, TEST_PUBKEY_HEX);
    // Self-authored note dropped → only the two foreign notes remain.
    assert_eq!(notes.len(), 2);
    assert!(notes.iter().all(|n| n.id != "self-note"));
    // Newest-first ordering.
    assert_eq!(notes[0].id, "newer-foreign");
    assert_eq!(notes[1].id, "older-foreign");
}

#[test]
fn parse_note_falls_back_to_first_e_tag_when_unmarked() {
    let first = "3333333333333333333333333333333333333333333333333333333333333333";
    let ev = serde_json::json!({
        "id": "reply-id",
        "pubkey": PEER_PUBKEY_HEX,
        "content": "re: episode 42",
        "created_at": 2,
        "tags": [
            ["e", first],
            ["e", "4444444444444444444444444444444444444444444444444444444444444444"],
            ["p", TEST_PUBKEY_HEX],
        ],
    });
    let note = parse_agent_note(&ev).unwrap();
    assert_eq!(note.root_event_id.as_deref(), Some(first));
}
