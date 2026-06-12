//! Social substate — Step 10 of the god-root consolidation.
//!
//! Owns the two slots that were previously mirrored between
//! `PodcastHandle` and `PodcastHostOpHandler`:
//!
//! * `social` — `Option<SocialSnapshot>` from the reactive NIP-02 follow list.
//!   **Session** durability (cleared on account switch).
//! * `agent_notes` — `Vec<CachedAgentNote>` from inbound kind:1 agent notes.
//!   **Session** durability (cleared on account switch).
//!
//! ## Observer wiring (dead-duplicate removal)
//!
//! `AgentNotesObserver` (in `crate::agent_note_handler`) writes `agent_notes`
//! off the actor thread.  It obtains its Arc via `state.social.agent_notes.share()`
//! at registration time in `register.rs`.
//!
//! `social` is written by the reactive `FollowListObserver` (in
//! `crate::social_handler`) on every kind:3 push frame, which obtains its Arc
//! via `state.social.social_slot.share()`.
//!
//! ## Trust gate — live at projection
//!
//! `agent_notes` caches notes as [`CachedAgentNote`] with the author **hex**
//! retained and NO `trusted` stamp. [`SocialState::agent_notes_snapshot`]
//! recomputes the trust verdict at build time by applying the shared live
//! `ActiveFollowSet` predicate to each note's author hex — so follow/unfollow
//! flips the verdict on ALL existing notes immediately, with no stale freeze.
//!
//! The `ActiveFollowSet` Arc is injected via [`SocialState::with_follow_set`]
//! at registration; it is the SAME Arc registered as a `KernelEventObserver`,
//! so the predicate always reflects the latest kind:3.
//!
//! ## File-length ceiling
//!
//! AGENTS.md hard limit is 500 lines.

use std::sync::Arc;

use nmp_nip02::ActiveFollowSet;

use crate::agent_note_handler::CachedAgentNote;
use crate::ffi::projections::{AgentNoteSummary, SocialSnapshot};
use crate::state::slot::Session;
use crate::state::{Infra, Slot};

/// Social feature substate.
///
/// Constructed once in `PodcastAppState::new` and referenced via
/// `state.social` on both seams.  All methods are `&self`.
pub struct SocialState {
    /// NIP-02 social graph snapshot.  `None` until the first kind:3 arrives
    /// reactively.  Written by `FollowListObserver` via `.share()`; read by
    /// the snapshot projection. Cleared on account switch.
    pub social_slot: Slot<Option<SocialSnapshot>, Session>,
    /// Inbound kind:1 agent-to-agent notes (raw cache, author hex retained,
    /// NO trust stamp).  Written by `AgentNotesObserver` off the actor thread
    /// via `.share()`; projected — with a live trust verdict — by
    /// [`Self::agent_notes_snapshot`]. Cleared on account switch.
    pub agent_notes: Slot<Vec<CachedAgentNote>, Session>,
    /// Live NIP-02 follow set, shared with the kernel observer registry.
    /// `None` in unit-test / legacy paths → every note projects `trusted:
    /// false` (fail-closed, D6).  Set via [`Self::with_follow_set`] at
    /// registration.
    follow_set: Option<Arc<ActiveFollowSet>>,
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
            follow_set: None,
            infra,
        }
    }

    /// Inject the shared live [`ActiveFollowSet`] so the trust verdict is
    /// recomputed at every projection build. Called from `register.rs` with
    /// the same Arc registered as a `KernelEventObserver`.
    pub fn with_follow_set(mut self, follow_set: Arc<ActiveFollowSet>) -> Self {
        self.follow_set = Some(follow_set);
        self
    }

    /// Test constructor.
    #[cfg(test)]
    pub fn for_test() -> Self {
        Self::new(Infra::for_test())
    }

    // ── Account switch ────────────────────────────────────────────────────

    /// Clear all per-account social state — the follow-list snapshot and the
    /// cached agent notes.  Called from the identity-change hook in
    /// `register.rs` so no cross-account state survives an A→B switch (A's
    /// following list / A's notes must not bleed into B's session).
    pub fn clear_for_account_switch(&self) {
        if let Ok(mut s) = self.social_slot.lock() {
            *s = None;
        }
        if let Ok(mut n) = self.agent_notes.lock() {
            n.clear();
        }
    }

    // ── Snapshot projections ──────────────────────────────────────────────

    /// Clone the current social snapshot for projection.
    pub fn social_snapshot(&self) -> Option<SocialSnapshot> {
        self.social_slot.lock().ok().and_then(|s| s.clone())
    }

    /// Project the cached agent notes into wire DTOs, computing `trusted`
    /// **live** against the shared `ActiveFollowSet` at build time.
    ///
    /// A note is `trusted` iff its author hex is in the active account's NIP-02
    /// follow set at the moment of projection — so a follow/unfollow flips the
    /// verdict on every existing note immediately. Fail-closed: with no
    /// follow set wired (tests) or a poisoned lock, the predicate returns
    /// `false` (D6).
    pub fn agent_notes_snapshot(&self) -> Vec<AgentNoteSummary> {
        let cached = match self.agent_notes.lock() {
            Ok(n) => n.clone(),
            Err(_) => return Vec::new(),
        };
        // Build the predicate ONCE per projection (it clones the inner
        // Arc<RwLock<BTreeSet>>; the lock is read inside the closure per call).
        let predicate = self.follow_set.as_ref().map(|fs| fs.predicate());
        cached
            .into_iter()
            .map(|note| {
                let trusted = predicate
                    .as_ref()
                    .map(|p| p(&note.author_hex))
                    .unwrap_or(false);
                AgentNoteSummary {
                    id: note.id,
                    author_npub: note.author_npub,
                    content: note.content,
                    created_at: note.created_at,
                    root_event_id: note.root_event_id,
                    trusted,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_note_handler::CachedAgentNote;
    use crate::ffi::projections::SocialSnapshot;
    use nmp_core::substrate::KernelEvent;
    use nmp_core::KernelEventObserver;
    use std::sync::Mutex;

    /// A valid-looking 64-hex pubkey for `author_hex` fields.
    const AUTHOR_X_HEX: &str =
        "aa11223344556677889900aabbccddeeff00112233445566778899aabbccddee";

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
            guard.push(cached_note("note1", AUTHOR_X_HEX));
        }
        assert_eq!(state.agent_notes_snapshot().len(), 1);
    }

    #[test]
    fn agent_notes_default_untrusted_without_follow_set() {
        // No ActiveFollowSet wired (test path) → every note projects
        // trusted:false (fail-closed).
        let state = SocialState::for_test();
        state
            .agent_notes
            .lock()
            .unwrap()
            .push(cached_note("note1", AUTHOR_X_HEX));
        let projected = state.agent_notes_snapshot();
        assert_eq!(projected.len(), 1);
        assert!(!projected[0].trusted);
    }

    #[test]
    fn clear_for_account_switch_empties_both_slots() {
        let state = SocialState::for_test();
        *state.social_slot.lock().unwrap() = Some(SocialSnapshot {
            following: vec![],
            following_count: 2,
        });
        state
            .agent_notes
            .lock()
            .unwrap()
            .push(cached_note("note1", AUTHOR_X_HEX));

        state.clear_for_account_switch();

        assert!(state.social_snapshot().is_none());
        assert!(state.agent_notes_snapshot().is_empty());
    }

    /// THE behavioral trust test: a note from X received BEFORE following X
    /// starts untrusted, and flips to trusted on the very next projection
    /// after the active-account kind:3 follows X — proving the verdict is
    /// computed live at projection, not frozen at receipt.
    #[test]
    fn existing_note_becomes_trusted_after_following_author() {
        // Active account.
        let me = "bb11223344556677889900aabbccddeeff00112233445566778899aabbccddee";
        let active_slot = Arc::new(Mutex::new(Some(me.to_string())));
        let follow_set = ActiveFollowSet::new(Arc::clone(&active_slot));

        let state = SocialState::for_test().with_follow_set(Arc::clone(&follow_set));

        // Step 1: a kind:1 note from X is cached (X not yet followed).
        state
            .agent_notes
            .lock()
            .unwrap()
            .push(cached_note("noteX", AUTHOR_X_HEX));

        let before = state.agent_notes_snapshot();
        assert_eq!(before.len(), 1);
        assert!(
            !before[0].trusted,
            "note from an unfollowed author must be untrusted"
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
        };
        follow_set.on_kernel_event(&kind3);

        // Step 3: re-project. The SAME existing note must now be trusted —
        // no new receipt, no cache mutation, purely projection-time recompute.
        let after = state.agent_notes_snapshot();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].id, "noteX");
        assert!(
            after[0].trusted,
            "existing note must flip to trusted once its author is followed"
        );
    }
}
