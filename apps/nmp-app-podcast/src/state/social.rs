//! Social substate — Step 10 of the god-root consolidation.
//!
//! Owns the two slots that were previously mirrored between
//! `PodcastHandle` and `PodcastHostOpHandler`:
//!
//! * `social` — `Option<SocialSnapshot>` from NIP-02 follow-list fetch.
//!   **Session** durability (re-fetched on demand via `FetchContacts`).
//! * `agent_notes` — `Vec<AgentNoteSummary>` from inbound kind:1 agent notes.
//!   **Session** durability (re-fetched on demand via `FetchAgentNotes`).
//!
//! ## Observer wiring (dead-duplicate removal)
//!
//! `AgentNotesObserver` (in `crate::agent_note_handler`) writes `agent_notes`
//! off the actor thread.  It obtains its Arc via `state.social.agent_notes.share()`
//! at registration time in `register.rs`.
//!
//! **Dead-duplicate removal**: the previous `PodcastHostOpHandler.agent_notes`
//! Arc was a dead clone — never read by the handler itself; the live write path
//! was always the observer's own Arc from `register.rs`.  Removing the handler
//! field (same PR this substate is added) is the natural outcome.
//!
//! `social` is written by `handle_fetch_contacts` via a tokio task, which
//! obtains its Arc via `state.social.social_slot.share()`.
//!
//! ## File-length ceiling
//!
//! AGENTS.md hard limit is 500 lines.

use crate::ffi::projections::{AgentNoteSummary, SocialSnapshot};
use crate::state::slot::Session;
use crate::state::{Infra, Slot};

/// Social feature substate.
///
/// Constructed once in `PodcastAppState::new` and referenced via
/// `state.social` on both seams.  All methods are `&self`.
pub struct SocialState {
    /// NIP-02 social graph snapshot.  `None` until the first `FetchContacts`
    /// dispatch completes.  Written by the tokio task inside
    /// `handle_fetch_contacts` via `.share()`; read by the snapshot projection.
    pub social_slot: Slot<Option<SocialSnapshot>, Session>,
    /// Inbound kind:1 agent-to-agent notes.  Written by `AgentNotesObserver`
    /// off the actor thread via `.share()`; read by the snapshot projection.
    pub agent_notes: Slot<Vec<AgentNoteSummary>, Session>,
    /// Rev + signal + runtime (cloned from `PodcastAppState::infra`).
    /// Kept for future bump-on-write; suppressed until first use.
    #[allow(dead_code)]
    pub(crate) infra: Infra,
}

impl SocialState {
    /// Production constructor — called from `PodcastAppState::new`.
    pub fn new(infra: Infra) -> Self {
        Self {
            social_slot: Slot::new(None),
            agent_notes: Slot::new(Vec::new()),
            infra,
        }
    }

    /// Test constructor.
    #[cfg(test)]
    pub fn for_test() -> Self {
        Self::new(Infra::for_test())
    }

    // ── Snapshot projections ──────────────────────────────────────────────

    /// Clone the current social snapshot for projection.
    pub fn social_snapshot(&self) -> Option<SocialSnapshot> {
        self.social_slot.lock().ok().and_then(|s| s.clone())
    }

    /// Clone the current agent-notes list for projection.
    pub fn agent_notes_snapshot(&self) -> Vec<AgentNoteSummary> {
        self.agent_notes
            .lock()
            .ok()
            .map(|n| n.clone())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::projections::{AgentNoteSummary, SocialSnapshot};

    #[test]
    fn social_snapshot_none_on_init() {
        let state = SocialState::for_test();
        assert!(state.social_snapshot().is_none());
    }

    #[test]
    fn agent_notes_empty_on_init() {
        let state = SocialState::for_test();
        assert!(state.agent_notes_snapshot().is_empty());
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
            });
        }
        let snap = state.social_snapshot().unwrap();
        assert_eq!(snap.following_count, 3);
    }

    #[test]
    fn agent_notes_share_is_same_arc() {
        let state = SocialState::for_test();
        let shared = state.agent_notes.share();
        {
            let mut guard = shared.lock().unwrap();
            guard.push(AgentNoteSummary {
                id: "note1".into(),
                author_npub: "npub1".into(),
                content: "hello".into(),
                created_at: 0,
                root_event_id: None,
                trusted: false,
            });
        }
        assert_eq!(state.agent_notes_snapshot().len(), 1);
    }
}
