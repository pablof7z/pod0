//! Conversation-slot and snapshot tests for [`super::SocialState`].
//!
//! Covers: social_snapshot_*, agent_notes_*, nostr_conversations_*,
//! clear_for_account_switch_*, and the live-trust-recompute behavioral test.
//!
//! Split from `social.rs` to keep every file under the 500-line hard limit
//! (AGENTS.md).

use super::*;
use crate::agent_note_handler::CachedAgentNote;
use crate::ffi::projections::SocialSnapshot;
use crate::store::outbound_turn_cache::OutboundTurn;
use nmp_core::substrate::KernelEvent;
use nmp_core::ObservedProjectionSink;
use nmp_nip02::{ActiveFollowSet, LatestKind3FollowSet};
use std::sync::{Arc, Mutex};

/// A valid-looking 64-hex pubkey for `author_hex` fields.
const AUTHOR_X_HEX: &str = "aa11223344556677889900aabbccddeeff00112233445566778899aabbccddee";

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

#[test]
fn social_snapshot_none_on_init() {
    let state = SocialState::for_test();
    assert!(state.social_snapshot().is_none());
}

#[test]
fn agent_notes_cache_empty_on_init() {
    // The inbound agent_notes cache (feeds nostr_conversations) is empty at init.
    let state = SocialState::for_test();
    assert!(state.agent_notes.lock().unwrap().is_empty());
    // conversations projection is also empty.
    assert!(state.nostr_conversations_snapshot().is_empty());
}

#[test]
fn social_share_is_same_arc() {
    let state = SocialState::for_test();
    let shared = state.social_slot.share();
    {
        let mut guard = shared.lock().unwrap();
        *guard = Some(SocialSnapshot {
            following: vec![],
            following_count: 3,
            approved_pubkeys: Vec::new(),
            blocked_pubkeys: Vec::new(),
        });
    }
    let snap = state.social_snapshot().unwrap();
    assert_eq!(snap.following_count, 3);
}

#[test]
fn agent_notes_share_is_same_arc() {
    // Verify that .share() produces the same Arc as the internal cache —
    // pushing via the shared handle is visible to nostr_conversations_snapshot.
    let state = SocialState::for_test();
    let shared = state.agent_notes.share();
    {
        let mut guard = shared.lock().unwrap();
        guard.push(cached_note("note1", AUTHOR_X_HEX));
    }
    // The note must surface via the conversations projection (the flat
    // agent_notes_snapshot was retired; conversations are the canonical read).
    let convs = state.nostr_conversations_snapshot();
    assert_eq!(
        convs.len(),
        1,
        "shared arc push must be visible to conversations"
    );
}

#[test]
fn inbound_note_default_untrusted_without_follow_set() {
    // No ActiveFollowSet wired (test path) → every conversation projects
    // trusted:false (fail-closed, D6).
    let state = SocialState::for_test();
    state
        .agent_notes
        .lock()
        .unwrap()
        .push(cached_note("note1", AUTHOR_X_HEX));
    let convs = state.nostr_conversations_snapshot();
    assert_eq!(convs.len(), 1);
    assert!(
        !convs[0].trusted,
        "without a follow set conversations must be untrusted"
    );
}

#[test]
fn clear_for_account_switch_empties_both_slots() {
    let state = SocialState::for_test();
    *state.social_slot.lock().unwrap() = Some(SocialSnapshot {
        following: vec![],
        following_count: 2,
        approved_pubkeys: Vec::new(),
        blocked_pubkeys: Vec::new(),
    });
    state
        .agent_notes
        .lock()
        .unwrap()
        .push(cached_note("note1", AUTHOR_X_HEX));

    state.clear_for_account_switch();

    assert!(state.social_snapshot().is_none());
    // The inbound notes cache (which feeds conversations) must also clear.
    assert!(state.agent_notes.lock().unwrap().is_empty());
    assert!(state.nostr_conversations_snapshot().is_empty());
}

#[test]
fn clear_for_account_switch_also_empties_outbound_turns() {
    let state = SocialState::for_test();
    state.record_outbound_turn(OutboundTurn {
        event_id: "out1".into(),
        root_event_id: "root1".into(),
        counterparty_hex: AUTHOR_X_HEX.into(),
        content: "hi".into(),
        created_at: 1_000,
    });
    assert_eq!(state.outbound_turns.lock().unwrap().len(), 1);
    state.clear_for_account_switch();
    assert!(state.outbound_turns.lock().unwrap().is_empty());
}

#[test]
fn nostr_conversations_empty_on_init() {
    let state = SocialState::for_test();
    assert!(state.nostr_conversations_snapshot().is_empty());
}

#[test]
fn nostr_conversations_groups_inbound_by_root() {
    let state = SocialState::for_test();
    // Two notes in the same root thread.
    let mut note1 = cached_note("n1", AUTHOR_X_HEX);
    note1.created_at = 100;
    note1.root_event_id = None; // n1 is the root
    let note2 = CachedAgentNote {
        id: "n2".into(),
        author_hex: AUTHOR_X_HEX.into(),
        author_npub: "npub_x".into(),
        content: "reply".into(),
        created_at: 200,
        root_event_id: Some("n1".into()),
    };
    state.agent_notes.lock().unwrap().push(note1);
    state.agent_notes.lock().unwrap().push(note2);

    let convs = state.nostr_conversations_snapshot();
    assert_eq!(convs.len(), 1, "both notes should form one conversation");
    let conv = &convs[0];
    assert_eq!(conv.root_event_id, "n1");
    assert_eq!(conv.turns.len(), 2);
    // Turns sorted ascending by created_at.
    assert_eq!(conv.turns[0].event_id, "n1");
    assert_eq!(conv.turns[1].event_id, "n2");
    assert_eq!(conv.first_seen, 100);
    assert_eq!(conv.last_activity, 200);
    // No follow set wired → untrusted (fail-closed).
    assert!(!conv.trusted);
}

#[test]
fn nostr_conversations_merges_outbound_turns() {
    let state = SocialState::for_test();
    // Inbound note opens the thread.
    let mut inbound = cached_note("n1", AUTHOR_X_HEX);
    inbound.created_at = 100;
    state.agent_notes.lock().unwrap().push(inbound);

    // Outbound reply into the same root.
    state.record_outbound_turn(OutboundTurn {
        event_id: "out1".into(),
        root_event_id: "n1".into(),
        counterparty_hex: AUTHOR_X_HEX.into(),
        content: "my reply".into(),
        created_at: 200,
    });

    let convs = state.nostr_conversations_snapshot();
    assert_eq!(convs.len(), 1);
    let conv = &convs[0];
    assert_eq!(conv.turns.len(), 2);
    // inbound first, outbound second (created_at ascending).
    assert_eq!(conv.turns[0].direction, "inbound");
    assert_eq!(conv.turns[1].direction, "outbound");
}

#[test]
fn nostr_conversations_sorted_newest_first() {
    let state = SocialState::for_test();
    // Thread A: recent (created_at=500).
    let mut na = cached_note("na", AUTHOR_X_HEX);
    na.created_at = 500;
    // Thread B (different root, older).
    let peer2 = "cc11223344556677889900aabbccddeeff00112233445566778899aabbccddee";
    let mut nb = cached_note("nb", peer2);
    nb.created_at = 100;
    state.agent_notes.lock().unwrap().push(na);
    state.agent_notes.lock().unwrap().push(nb);

    let convs = state.nostr_conversations_snapshot();
    assert_eq!(convs.len(), 2);
    // Thread A has last_activity=500; it should be first.
    assert_eq!(convs[0].root_event_id, "na");
    assert_eq!(convs[1].root_event_id, "nb");
}

/// THE behavioral trust test: a note from X received BEFORE following X
/// starts untrusted, and flips to trusted on the very next projection
/// after the active-account kind:3 follows X — proving the verdict is
/// computed live at projection, not frozen at receipt.
///
/// Verified via `nostr_conversations_snapshot` (the canonical projection
/// since the flat `agent_notes_snapshot` was retired).
#[test]
fn existing_note_becomes_trusted_after_following_author() {
    // Active account.
    let me = "bb11223344556677889900aabbccddeeff00112233445566778899aabbccddee";
    let active_slot = Arc::new(Mutex::new(Some(me.to_string())));
    let follow_set = ActiveFollowSet::new(
        Arc::clone(&active_slot),
        LatestKind3FollowSet::new(nmp_core::slots::new_event_store_slot()),
    );

    let state = SocialState::for_test().with_follow_set(Arc::clone(&follow_set));

    // Step 1: a kind:1 note from X is cached (X not yet followed).
    state
        .agent_notes
        .lock()
        .unwrap()
        .push(cached_note("noteX", AUTHOR_X_HEX));

    let before = state.nostr_conversations_snapshot();
    assert_eq!(before.len(), 1);
    assert!(
        !before[0].trusted,
        "conversation from an unfollowed author must be untrusted"
    );

    // Step 2: the active account publishes a kind:3 FOLLOWING X. Drive the
    // ActiveFollowSet observer directly (no relay) so the set updates.
    let kind3 = KernelEvent {
        id: nmp_core::substrate::EventId::from(
            "0000000000000000000000000000000000000000000000000000000000000001".to_string(),
        ),
        author: me.to_string(),
        kind: 3,
        created_at: 100,
        tags: vec![vec!["p".to_string(), AUTHOR_X_HEX.to_string()]],
        content: String::new(),
        relay_provenance: vec![],
    };
    follow_set.on_kernel_event(&kind3);

    // Step 3: re-project. The SAME existing conversation must now be trusted —
    // no new receipt, no cache mutation, purely projection-time recompute.
    let after = state.nostr_conversations_snapshot();
    assert_eq!(after.len(), 1);
    // The root_event_id for a rootless note equals the note's id.
    assert_eq!(after[0].root_event_id, "noteX");
    assert!(
        after[0].trusted,
        "existing conversation must flip to trusted once its author is followed"
    );
}
