//! In-memory state machine for agent-chat conversations + approvals.
//!
//! [`ConversationActor`] is the pure-data owner of every active
//! [`NostrConversation`] and outstanding [`PendingApproval`]. It exposes
//! a narrow imperative API the kernel-side `ActionModule` impls invoke
//! when an agent action lands; persistence + FFI snapshot serialization
//! live one layer up (M7.B for the action modules, this milestone for
//! the snapshot dataclass).
//!
//! ## Doctrine
//!
//! * **Pure** — no async, no network, no `Utc::now()` inside mutating
//!   methods that take their timestamps from the caller. Tests can drive
//!   the state machine deterministically.
//! * **Borrow-free outputs** — every accessor returns either an owned
//!   value or a `Vec<Owned>`, so the projection layer can hand results
//!   to serde without lifetime juggling.

use std::collections::HashMap;

use uuid::Uuid;

use crate::types::{
    ApprovalDecision, NostrConversation, NostrConversationTurn, PendingApproval,
};

/// State machine for active conversations and pending approvals.
#[derive(Clone, Debug, Default)]
pub struct ConversationActor {
    conversations: HashMap<Uuid, NostrConversation>,
    pending_approvals: HashMap<Uuid, PendingApproval>,
    /// Most recently touched conversation id — surfaced into the
    /// snapshot so the UI can highlight "active" without re-sorting
    /// every tick.
    latest_conversation_id: Option<Uuid>,
}

impl ConversationActor {
    pub fn new() -> Self {
        Self::default()
    }

    // ── conversations ────────────────────────────────────────────────

    /// Mint a fresh empty conversation and return its id. The actor
    /// keeps the resulting [`NostrConversation`] in its store.
    pub fn new_conversation(&mut self) -> Uuid {
        let convo = NostrConversation::new();
        let id = convo.id;
        self.conversations.insert(id, convo);
        self.latest_conversation_id = Some(id);
        id
    }

    /// Append `turn` to `conversation_id`. Silently no-ops when the
    /// conversation isn't known — the kernel-side action module is
    /// responsible for minting before send.
    pub fn add_turn(&mut self, conversation_id: Uuid, turn: NostrConversationTurn) {
        if let Some(c) = self.conversations.get_mut(&conversation_id) {
            c.push(turn);
            self.latest_conversation_id = Some(conversation_id);
        }
    }

    /// Set a conversation's `title` if it exists. No-op on miss.
    pub fn set_title(&mut self, conversation_id: Uuid, title: impl Into<String>) {
        if let Some(c) = self.conversations.get_mut(&conversation_id) {
            c.title = Some(title.into());
        }
    }

    /// Drop every turn but keep the [`NostrConversation`] row (id +
    /// timestamps remain so the UI can reuse the slot).
    pub fn clear_conversation(&mut self, conversation_id: Uuid) {
        if let Some(c) = self.conversations.get_mut(&conversation_id) {
            c.turns.clear();
            c.title = None;
        }
    }

    /// Return the last `max_turns` turns of `conversation_id`, in
    /// timestamp order. Returns an empty `Vec` when the conversation
    /// isn't known.
    pub fn conversation_context(
        &self,
        id: Uuid,
        max_turns: usize,
    ) -> Vec<NostrConversationTurn> {
        let Some(c) = self.conversations.get(&id) else {
            return Vec::new();
        };
        if max_turns == 0 || c.turns.is_empty() {
            return Vec::new();
        }
        let start = c.turns.len().saturating_sub(max_turns);
        c.turns[start..].to_vec()
    }

    /// Snapshot accessor: how many conversations live in the store.
    pub fn active_count(&self) -> usize {
        self.conversations.len()
    }

    /// Snapshot accessor: most recently touched conversation id (if any).
    pub fn latest_conversation_id(&self) -> Option<Uuid> {
        self.latest_conversation_id
    }

    /// Borrowing accessor used by the snapshot builder. Returns
    /// `None` when the conversation has been cleared/dropped.
    pub fn conversation(&self, id: Uuid) -> Option<&NostrConversation> {
        self.conversations.get(&id)
    }

    // ── approvals ────────────────────────────────────────────────────

    /// Park a new pending approval. If an approval with the same id was
    /// already parked it gets replaced (the kernel layer guards against
    /// id collisions; the actor itself is tolerant).
    pub fn add_approval(&mut self, approval: PendingApproval) {
        self.pending_approvals.insert(approval.id, approval);
    }

    /// Resolve a pending approval and return the recorded decision. The
    /// approval is removed from the pending set regardless of decision —
    /// the caller is responsible for fanning out the side effect.
    /// Returns `None` if the approval id wasn't pending.
    pub fn decide_approval(
        &mut self,
        approval_id: Uuid,
        decision: ApprovalDecision,
    ) -> Option<(PendingApproval, ApprovalDecision)> {
        self.pending_approvals
            .remove(&approval_id)
            .map(|a| (a, decision))
    }

    /// Read-only iteration over outstanding approvals, in insertion-
    /// undefined order. Callers that need stable ordering should sort by
    /// `requested_at`.
    pub fn pending_approvals(&self) -> Vec<PendingApproval> {
        self.pending_approvals.values().cloned().collect()
    }

    /// How many approvals are currently parked.
    pub fn pending_count(&self) -> usize {
        self.pending_approvals.len()
    }
}

#[cfg(test)]
#[path = "conversations_tests.rs"]
mod tests;
