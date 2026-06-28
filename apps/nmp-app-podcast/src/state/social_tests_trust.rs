//! Trust-predicate and approve/block tests for [`super::SocialState`].
//!
//! Covers: trust_predicate_*, approve_peer_*, block_peer_*,
//! conversation_*_peer_* flags, and the fail-closed poisoned-lock guard.
//!
//! Split from `social.rs` to keep every file under the 500-line hard limit
//! (AGENTS.md).

use super::*;
use crate::agent_note_handler::CachedAgentNote;
use crate::store::approved_peer_store::ApprovedPeerStore;
use nmp_core::substrate::KernelEvent;
use nmp_core::KernelEventObserver;
use nmp_nip02::ActiveFollowSet;
use std::sync::{Arc, Mutex};

fn cached_note(id: &str, author_hex: &str) -> CachedAgentNote {
    CachedAgentNote {
        id: id.into(),
        author_hex: author_hex.into(),
        author_npub: format!("npub_for_{author_hex}"),
        content: "hello".into(),
        created_at: 0,
        root_event_id: None,
    }
}

fn make_follow_set_with_member(me: &str, member_hex: &str) -> Arc<ActiveFollowSet> {
    // ActiveFollowSet::new already returns Arc<ActiveFollowSet>.
    let active_slot = Arc::new(Mutex::new(Some(me.to_string())));
    let follow_set = ActiveFollowSet::new(Arc::clone(&active_slot));
    let kind3 = KernelEvent {
        id: nmp_core::substrate::EventId::from(
            "0000000000000000000000000000000000000000000000000000000000000002".to_string(),
        ),
        author: me.to_string(),
        kind: 3,
        created_at: 200,
        tags: vec![vec!["p".to_string(), member_hex.to_string()]],
        content: String::new(),
        relay_provenance: vec![],
    };
    follow_set.on_kernel_event(&kind3);
    follow_set
}

const ME: &str = "ee11223344556677889900aabbccddeeff00112233445566778899aabbccddee";
const PEER: &str = "aa11223344556677889900aabbccddeeff00112233445566778899aabbccddee";
const OTHER: &str = "ff11223344556677889900aabbccddeeff00112233445566778899aabbccddee";

/// followed-only, not approved, not blocked → trusted
#[test]
fn trust_predicate_followed_only_is_trusted() {
    let follow_set = make_follow_set_with_member(ME, PEER);
    let state = SocialState::for_test().with_follow_set(follow_set);
    let pred = state.trust_predicate();
    assert!(pred(PEER), "followed-only must be trusted");
}

/// approved-only, not followed, not blocked → trusted
#[test]
fn trust_predicate_approved_only_is_trusted() {
    let approved_store = Arc::new(Mutex::new(ApprovedPeerStore::new()));
    approved_store.lock().unwrap().approve(PEER);
    let state = SocialState::for_test().with_approved_peers(approved_store);
    let pred = state.trust_predicate();
    assert!(pred(PEER), "approved-only must be trusted");
}

/// not followed, not approved → untrusted
#[test]
fn trust_predicate_neither_is_untrusted() {
    let state = SocialState::for_test();
    let pred = state.trust_predicate();
    assert!(!pred(PEER), "neither followed nor approved must be untrusted");
}

/// blocked overrides follow → untrusted
#[test]
fn trust_predicate_block_overrides_follow() {
    let follow_set = make_follow_set_with_member(ME, PEER);
    let approved_store = Arc::new(Mutex::new(ApprovedPeerStore::new()));
    approved_store.lock().unwrap().block(PEER);
    let state = SocialState::for_test()
        .with_follow_set(follow_set)
        .with_approved_peers(approved_store);
    let pred = state.trust_predicate();
    assert!(!pred(PEER), "blocked must override follow");
}

/// blocked overrides explicit approval → untrusted
#[test]
fn trust_predicate_block_overrides_approval() {
    let approved_store = Arc::new(Mutex::new(ApprovedPeerStore::new()));
    {
        let mut s = approved_store.lock().unwrap();
        s.approve(PEER);
        s.block(PEER); // block clears the approval
    }
    let state = SocialState::for_test().with_approved_peers(approved_store);
    let pred = state.trust_predicate();
    assert!(!pred(PEER), "block must override approval");
}

/// followed+approved, different peer blocked → followed peer still trusted
#[test]
fn trust_predicate_unrelated_block_does_not_affect_other_peer() {
    let follow_set = make_follow_set_with_member(ME, PEER);
    let approved_store = Arc::new(Mutex::new(ApprovedPeerStore::new()));
    approved_store.lock().unwrap().block(OTHER);
    let state = SocialState::for_test()
        .with_follow_set(follow_set)
        .with_approved_peers(approved_store);
    let pred = state.trust_predicate();
    assert!(pred(PEER), "blocking OTHER must not affect PEER trust");
    assert!(!pred(OTHER), "OTHER must remain blocked");
}

/// `approve_peer` / `block_peer` mutation helpers change projection live
/// (verified via `nostr_conversations_snapshot` — the canonical projection
/// since the flat `agent_notes_snapshot` was retired).
#[test]
fn approve_peer_flips_conversation_trusted_to_true() {
    let approved_store = Arc::new(Mutex::new(ApprovedPeerStore::new()));
    let state = SocialState::for_test().with_approved_peers(approved_store);
    // Seed an inbound note from PEER.
    state
        .agent_notes
        .lock()
        .unwrap()
        .push(cached_note("noteA", PEER));

    // Before approve: untrusted (no follow, no approve).
    let before = state.nostr_conversations_snapshot();
    assert!(!before[0].trusted, "must be untrusted before approve");

    // Approve via mutating helper.
    state.approve_peer(PEER);

    // After approve: trusted.
    let after = state.nostr_conversations_snapshot();
    assert!(after[0].trusted, "must be trusted after approve");
}

/// `block_peer` overrides a follow in the live projection
#[test]
fn block_peer_overrides_follow_in_projection() {
    let follow_set = make_follow_set_with_member(ME, PEER);
    let approved_store = Arc::new(Mutex::new(ApprovedPeerStore::new()));
    let state = SocialState::for_test()
        .with_follow_set(follow_set)
        .with_approved_peers(approved_store);
    state
        .agent_notes
        .lock()
        .unwrap()
        .push(cached_note("noteB", PEER));

    // Before block: trusted (followed).
    let before = state.nostr_conversations_snapshot();
    assert!(before[0].trusted, "must be trusted before block");

    // Block via mutating helper.
    state.block_peer(PEER);

    // After block: untrusted despite follow.
    let after = state.nostr_conversations_snapshot();
    assert!(!after[0].trusted, "must be untrusted after block despite follow");
}

/// A poisoned `approved` mutex must fail CLOSED: even a FOLLOWED peer must
/// become untrusted, because we can no longer read the blocklist to prove
/// they are not blocked. Dropping blocks here (fail-OPEN) would let a
/// blocked-but-followed peer be auto-replied to — the bug this guards.
#[test]
fn trust_predicate_fails_closed_on_poisoned_approved_lock() {
    let follow_set = make_follow_set_with_member(ME, PEER);
    let approved_store = Arc::new(Mutex::new(ApprovedPeerStore::new()));
    let state = SocialState::for_test()
        .with_follow_set(follow_set)
        .with_approved_peers(Arc::clone(&approved_store));

    // Sanity: followed peer trusted with a healthy lock.
    assert!(
        state.trust_predicate()(PEER),
        "followed peer must be trusted before poisoning"
    );

    // Poison the mutex: panic while holding the lock on another thread.
    let poison_arc = Arc::clone(&approved_store);
    let _ = std::thread::spawn(move || {
        let _guard = poison_arc.lock().unwrap();
        panic!("intentional panic to poison the approved-peer mutex");
    })
    .join();
    assert!(
        approved_store.lock().is_err(),
        "mutex must be poisoned for this test to be meaningful"
    );

    // Fail closed: even the FOLLOWED peer is now untrusted.
    let pred = state.trust_predicate();
    assert!(
        !pred(PEER),
        "poisoned approved lock must fail closed — followed peer becomes untrusted"
    );
    assert!(
        !pred(OTHER),
        "poisoned approved lock must deny everyone"
    );
}

// ── Explicit per-peer conversation flags (peer_blocked / peer_approved) ────
//
// These drive the Android conversation-trust state machine, which must
// distinguish blocked vs explicitly-approved vs follow-only — a distinction
// the composed `trusted` bool alone cannot express.

/// Blocked peer → `peer_blocked == true` AND composed `trusted == false`.
#[test]
fn conversation_blocked_peer_sets_peer_blocked_and_untrusted() {
    let approved_store = Arc::new(Mutex::new(ApprovedPeerStore::new()));
    approved_store.lock().unwrap().block(PEER);
    let state = SocialState::for_test().with_approved_peers(approved_store);
    state
        .agent_notes
        .lock()
        .unwrap()
        .push(cached_note("noteBlk", PEER));

    let conv = &state.nostr_conversations_snapshot()[0];
    assert!(conv.peer_blocked, "explicitly blocked peer must set peer_blocked");
    assert!(!conv.peer_approved, "blocked peer is not approved");
    assert!(!conv.trusted, "blocked peer must be untrusted");
}

/// Explicitly-approved (NOT followed) peer → `peer_approved == true` AND
/// composed `trusted == true`, with `peer_blocked == false`.
#[test]
fn conversation_approved_peer_sets_peer_approved_and_trusted() {
    let approved_store = Arc::new(Mutex::new(ApprovedPeerStore::new()));
    approved_store.lock().unwrap().approve(PEER);
    let state = SocialState::for_test().with_approved_peers(approved_store);
    state
        .agent_notes
        .lock()
        .unwrap()
        .push(cached_note("noteApp", PEER));

    let conv = &state.nostr_conversations_snapshot()[0];
    assert!(conv.peer_approved, "explicitly approved peer must set peer_approved");
    assert!(!conv.peer_blocked, "approved peer is not blocked");
    assert!(conv.trusted, "approved peer must be trusted");
}

/// Follow-only peer (no explicit approval/block) → `trusted == true` but
/// `peer_approved == false` and `peer_blocked == false`. This is the case
/// that makes a "Remove approval" action a no-op dead-end on the shell, so
/// the flags MUST distinguish it from explicit approval.
#[test]
fn conversation_follow_only_peer_is_trusted_but_not_explicitly_approved() {
    let follow_set = make_follow_set_with_member(ME, PEER);
    let state = SocialState::for_test().with_follow_set(follow_set);
    state
        .agent_notes
        .lock()
        .unwrap()
        .push(cached_note("noteFollow", PEER));

    let conv = &state.nostr_conversations_snapshot()[0];
    assert!(conv.trusted, "follow-only peer must be trusted");
    assert!(
        !conv.peer_approved,
        "follow-only trust must NOT report explicit approval"
    );
    assert!(!conv.peer_blocked, "follow-only peer is not blocked");
}
