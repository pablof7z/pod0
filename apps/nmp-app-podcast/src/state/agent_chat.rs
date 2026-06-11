//! AgentChat substate — Step 11 of the god-root consolidation.
//!
//! Owns the three slots previously mirrored between `PodcastHandle` and
//! `PodcastHostOpHandler`:
//!
//! * `conversation` — `Vec<AgentMessageSummary>` in-memory transcript.
//!   **Session** durability.
//! * `agent_busy` — `Arc<AtomicBool>` re-entrancy guard.  Session.
//! * `agent_touched` — `Arc<AtomicBool>` "was used this session" flag.  Session.
//!
//! ## Design choice: wrapping `AgentChatHandler`
//!
//! `AgentChatHandler` already composes these three `Arc`s (plus runtime /
//! store / signal) and exposes a `handle(AgentChatAction) -> Value` method.
//! Rather than duplicating that logic here, `AgentChatState` owns the
//! handler directly.  The slot `Arc`s are accessed via getter methods
//! (`conversation_arc`, `busy_arc`, `touched_arc`) for the snapshot reader.
//!
//! ## File-length ceiling
//!
//! AGENTS.md hard limit is 500 lines.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::agent_handler::AgentChatHandler;
use crate::ffi::actions::AgentChatAction;
use crate::ffi::projections::AgentMessageSummary;
use crate::state::Infra;
use crate::store::PodcastStore;

/// AgentChat feature substate.
///
/// Constructed once in `PodcastAppState::new` and referenced via
/// `state.agent_chat` on both seams.  All action dispatch goes through
/// `handle`; the snapshot reader calls `conversation_snapshot`, `is_busy`,
/// and `is_touched` to project the current state.
pub struct AgentChatState {
    /// The inner handler owns the `Arc`s and the LLM dispatch logic.
    /// Public so `register.rs` can attach the snapshot signal before handing
    /// it over to `PodcastAppState`.  Post-construction it is accessed only
    /// via the methods below.
    pub(crate) handler: AgentChatHandler,
    /// Bare `Arc<Mutex<…>>` clones kept for the snapshot reader so it can
    /// lock just the conversation without going through the handler's API.
    pub(crate) conversation: Arc<Mutex<Vec<AgentMessageSummary>>>,
    pub(crate) agent_busy: Arc<AtomicBool>,
    pub(crate) agent_touched: Arc<AtomicBool>,
}

impl AgentChatState {
    /// Production constructor — called from `PodcastAppState::new`.
    ///
    /// Reads `infra.signal` to wire the snapshot signal into the inner handler
    /// so background LLM tasks bump the snapshot rev when they complete.
    pub fn new(infra: Infra, store: Arc<Mutex<PodcastStore>>) -> Self {
        let conversation: Arc<Mutex<Vec<AgentMessageSummary>>> =
            Arc::new(Mutex::new(Vec::new()));
        let agent_busy = Arc::new(AtomicBool::new(false));
        let agent_touched = Arc::new(AtomicBool::new(false));

        let mut handler = AgentChatHandler::new(
            conversation.clone(),
            agent_busy.clone(),
            agent_touched.clone(),
            infra.rev.clone(),
            infra.runtime.clone(),
            store,
        );
        // Wire the snapshot signal when available (production path).
        if let Some(signal) = infra.signal.clone() {
            handler = handler.with_snapshot_signal(signal);
        }

        Self {
            handler,
            conversation,
            agent_busy,
            agent_touched,
        }
    }

    /// Test constructor — no live runtime; LLM calls fall back to the
    /// scaffold reply.
    #[cfg(test)]
    pub fn for_test() -> Self {
        let conversation: Arc<Mutex<Vec<AgentMessageSummary>>> =
            Arc::new(Mutex::new(Vec::new()));
        let agent_busy = Arc::new(AtomicBool::new(false));
        let agent_touched = Arc::new(AtomicBool::new(false));
        let rev = Arc::new(std::sync::atomic::AtomicU64::new(1));
        let handler = AgentChatHandler::new_without_runtime(
            conversation.clone(),
            agent_busy.clone(),
            agent_touched.clone(),
            rev,
        );
        Self {
            handler,
            conversation,
            agent_busy,
            agent_touched,
        }
    }

    // ── Action handler ────────────────────────────────────────────────────

    /// Route a typed [`AgentChatAction`] to the handler.
    ///
    /// Replaces the `self.agent_chat.handle(action)` call in the router.
    pub fn handle(&self, action: AgentChatAction) -> serde_json::Value {
        self.handler.handle(action)
    }

    // ── Snapshot projections ──────────────────────────────────────────────

    /// Clone the current conversation for snapshot projection.
    pub fn conversation_snapshot(&self) -> Vec<AgentMessageSummary> {
        self.conversation
            .lock()
            .ok()
            .map(|c| c.clone())
            .unwrap_or_default()
    }

    /// Whether the LLM background task is running.
    pub fn is_busy(&self) -> bool {
        self.agent_busy.load(Ordering::Relaxed)
    }

    /// Whether the user has interacted with the agent this session.
    pub fn is_touched(&self) -> bool {
        self.agent_touched.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::actions::AgentChatAction;

    #[test]
    fn conversation_empty_on_init() {
        let state = AgentChatState::for_test();
        assert!(state.conversation_snapshot().is_empty());
        assert!(!state.is_busy());
        assert!(!state.is_touched());
    }

    #[test]
    fn send_appends_messages() {
        let state = AgentChatState::for_test();
        let out = state.handle(AgentChatAction::Send {
            message: "hello".into(),
        });
        assert_eq!(out["ok"], true);
        // user + assistant placeholder
        assert!(state.conversation_snapshot().len() >= 1);
        assert!(state.is_touched());
    }

    #[test]
    fn clear_empties_conversation() {
        let state = AgentChatState::for_test();
        let _ = state.handle(AgentChatAction::Send {
            message: "hello".into(),
        });
        let out = state.handle(AgentChatAction::Clear);
        assert_eq!(out["ok"], true);
        assert!(state.conversation_snapshot().is_empty());
        // touched stays true after clear
        assert!(state.is_touched());
    }
}
