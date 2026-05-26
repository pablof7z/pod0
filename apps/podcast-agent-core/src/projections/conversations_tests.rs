//! Tests for [super::conversations] — ConversationActor turn management and approval flow.
//!
//! Extracted from `conversations.rs` to keep that file under the 500-line hard limit.

use super::*;
use crate::types::{ConversationRole, NostrConversationTurn, PendingApproval};
use chrono::{DateTime, Utc};

fn ts(secs: i64) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp(secs, 0).unwrap()
}

fn turn(role: ConversationRole, content: &str, secs: i64) -> NostrConversationTurn {
    NostrConversationTurn {
        id: Uuid::new_v4(),
        role,
        content: content.to_owned(),
        timestamp: ts(secs),
        metadata: None,
    }
}

#[test]
fn add_turn_grows_conversation() {
    let mut actor = ConversationActor::new();
    let id = actor.new_conversation();

    actor.add_turn(id, turn(ConversationRole::User, "hello", 1));
    actor.add_turn(id, turn(ConversationRole::Assistant, "hi there", 2));

    let convo = actor.conversation(id).expect("convo exists");
    assert_eq!(convo.turns.len(), 2);
    assert_eq!(convo.turns[0].content, "hello");
    assert_eq!(convo.turns[1].content, "hi there");
    assert_eq!(convo.updated_at, ts(2));
}

#[test]
fn add_turn_on_unknown_conversation_noops() {
    let mut actor = ConversationActor::new();
    let ghost = Uuid::new_v4();
    actor.add_turn(ghost, turn(ConversationRole::User, "x", 1));
    assert_eq!(actor.active_count(), 0);
    assert!(actor.conversation(ghost).is_none());
}

#[test]
fn conversation_context_returns_last_three_of_ten() {
    let mut actor = ConversationActor::new();
    let id = actor.new_conversation();
    for i in 0..10 {
        actor.add_turn(
            id,
            turn(ConversationRole::User, &format!("turn {i}"), i + 1),
        );
    }
    let ctx = actor.conversation_context(id, 3);
    assert_eq!(ctx.len(), 3);
    assert_eq!(ctx[0].content, "turn 7");
    assert_eq!(ctx[1].content, "turn 8");
    assert_eq!(ctx[2].content, "turn 9");
}

#[test]
fn conversation_context_handles_edges() {
    let mut actor = ConversationActor::new();
    let id = actor.new_conversation();

    // Empty conversation → empty context regardless of max_turns.
    assert!(actor.conversation_context(id, 5).is_empty());

    // max_turns=0 always empty.
    actor.add_turn(id, turn(ConversationRole::User, "x", 1));
    assert!(actor.conversation_context(id, 0).is_empty());

    // max_turns > len returns everything.
    let ctx = actor.conversation_context(id, 99);
    assert_eq!(ctx.len(), 1);

    // Unknown id returns empty.
    assert!(actor
        .conversation_context(Uuid::new_v4(), 3)
        .is_empty());
}

#[test]
fn clear_conversation_drops_turns_keeps_row() {
    let mut actor = ConversationActor::new();
    let id = actor.new_conversation();
    actor.set_title(id, "Old chat");
    actor.add_turn(id, turn(ConversationRole::User, "x", 1));
    actor.clear_conversation(id);

    let c = actor.conversation(id).expect("row still present");
    assert!(c.turns.is_empty());
    assert!(c.title.is_none());
}

#[test]
fn latest_conversation_id_tracks_most_recent_touch() {
    let mut actor = ConversationActor::new();
    let a = actor.new_conversation();
    let b = actor.new_conversation();
    assert_eq!(actor.latest_conversation_id(), Some(b));

    actor.add_turn(a, turn(ConversationRole::User, "x", 1));
    assert_eq!(actor.latest_conversation_id(), Some(a));
}

#[test]
fn decide_approval_removes_from_pending() {
    let mut actor = ConversationActor::new();
    let convo = actor.new_conversation();
    let ap = PendingApproval::new(convo, "publish");
    let ap_id = ap.id;
    actor.add_approval(ap);
    assert_eq!(actor.pending_count(), 1);

    let result = actor.decide_approval(ap_id, ApprovalDecision::Approved);
    assert!(result.is_some());
    assert_eq!(result.unwrap().1, ApprovalDecision::Approved);
    assert_eq!(actor.pending_count(), 0);

    // Second decide is a no-op.
    assert!(actor
        .decide_approval(ap_id, ApprovalDecision::Approved)
        .is_none());
}

#[test]
fn decide_approval_with_denial_carries_reason() {
    let mut actor = ConversationActor::new();
    let convo = actor.new_conversation();
    let ap = PendingApproval::new(convo, "publish");
    let ap_id = ap.id;
    actor.add_approval(ap);

    let decision = ApprovalDecision::Denied {
        reason: Some("not yet".into()),
    };
    let (taken, recorded) = actor
        .decide_approval(ap_id, decision.clone())
        .expect("decision");
    assert_eq!(taken.id, ap_id);
    assert_eq!(recorded, decision);
}

#[test]
fn pending_approvals_listing_reflects_state() {
    let mut actor = ConversationActor::new();
    let convo = actor.new_conversation();
    let ap1 = PendingApproval::new(convo, "publish");
    let ap2 = PendingApproval::new(convo, "delete");
    actor.add_approval(ap1.clone());
    actor.add_approval(ap2.clone());
    assert_eq!(actor.pending_approvals().len(), 2);

    actor.decide_approval(ap1.id, ApprovalDecision::Approved);
    let remaining = actor.pending_approvals();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, ap2.id);
}
