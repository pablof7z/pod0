//! Social substate — Step 10 of the god-root consolidation.
//!
//! Owns the three slots that were previously mirrored between
//! `PodcastHandle` and `PodcastHostOpHandler`, plus the new outbound-turn
//! cache for Nostr conversation projection:
//!
//! * `social` — `Option<SocialSnapshot>` from the reactive NIP-02 follow list.
//!   **Session** durability (cleared on account switch).
//! * `agent_notes` — `Vec<CachedAgentNote>` from inbound kind:1 agent notes.
//!   **Session** durability (cleared on account switch).
//! * `outbound_turns` — `Vec<OutboundTurn>` of kernel-published auto-reply
//!   turns, loaded from disk at init and appended in-session.
//!   **Session** durability (cleared on account switch).
//! * `approved` — `Arc<Mutex<ApprovedPeerStore>>` of explicit user
//!   approve/block decisions. **Durable** (NOT cleared on account switch;
//!   reloaded from the account-scoped data dir so decisions survive restarts).
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
//! ## Trust gate — composed predicate (follow OR approve, AND NOT block)
//!
//! `trust(pubkey) = (followed(pubkey) || approved(pubkey)) && !blocked(pubkey)`
//!
//! * `followed` is the live `ActiveFollowSet` predicate (reactive, NIP-02).
//! * `approved` is an explicit per-peer user decision persisted in
//!   `ApprovedPeerStore`. An approved-but-unfollowed sender IS auto-replied to.
//! * `blocked` is an absolute override: a followed+blocked pubkey is untrusted.
//!
//! Both projection paths (`agent_notes_snapshot` and
//! `nostr_conversations_snapshot`) use [`SocialState::trust_predicate`] which
//! builds the composed closure once per projection call.
//!
//! The `ActiveFollowSet` Arc is injected via [`SocialState::with_follow_set`]
//! at registration; the `ApprovedPeerStore` Arc is injected via
//! [`SocialState::with_approved_peers`]. Both are the SAME Arcs held in
//! `register.rs`, so the predicate always reflects the latest kind:3 AND
//! the latest approve/block action.
//!
//! ## File-length ceiling
//!
//! AGENTS.md hard limit is 500 lines.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use nmp_nip02::ActiveFollowSet;

use crate::agent_note_handler::CachedAgentNote;
use crate::ffi::projections::{AgentNoteSummary, NostrConversationDTO, NostrConversationTurnDTO, SocialSnapshot};
use crate::state::slot::Session;
use crate::state::{Infra, Slot};
use crate::store::approved_peer_store::ApprovedPeerStore;
use crate::store::outbound_turn_cache::OutboundTurn;

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
    /// Kernel-published outbound auto-reply turns, loaded from disk at init
    /// and appended in-session via [`Self::record_outbound_turn`].
    /// Cleared on account switch so cross-account turns don't leak.
    pub outbound_turns: Slot<Vec<OutboundTurn>, Session>,
    /// Live NIP-02 follow set, shared with the kernel observer registry.
    /// `None` in unit-test / legacy paths → every note projects `trusted:
    /// false` (fail-closed, D6).  Set via [`Self::with_follow_set`] at
    /// registration.
    follow_set: Option<Arc<ActiveFollowSet>>,
    /// Kernel-owned approve/block allow-list. `None` in unit-test / legacy
    /// paths → only follow-set trust applies.  Set via
    /// [`Self::with_approved_peers`] at registration.  Durable: NOT cleared
    /// on account switch; reloaded from disk in `data_dir.rs`.
    pub approved: Option<Arc<Mutex<ApprovedPeerStore>>>,
    /// Rev + signal + runtime (cloned from `PodcastAppState::infra`).
    pub(crate) infra: Infra,
}

impl SocialState {
    /// Production constructor — called from `PodcastAppState::new`.
    pub fn new(infra: Infra) -> Self {
        Self {
            social_slot: Slot::new(None),
            agent_notes: Slot::new(Vec::new()),
            outbound_turns: Slot::new(Vec::new()),
            follow_set: None,
            approved: None,
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

    /// Inject the shared [`ApprovedPeerStore`] so the composed trust predicate
    /// (`follow || approve`) AND NOT `block` is applied at every projection
    /// build. Called from `register.rs` after constructing the store Arc;
    /// the same Arc is stored on `PodcastHandle` for persistence by
    /// `data_dir.rs`.
    pub fn with_approved_peers(mut self, store: Arc<Mutex<ApprovedPeerStore>>) -> Self {
        self.approved = Some(store);
        self
    }

    // ── Trust predicate ───────────────────────────────────────────────────

    /// Build the composed trust predicate for one projection call.
    ///
    /// `trust(pubkey) = (followed(pubkey) || approved(pubkey)) && !blocked(pubkey)`
    ///
    /// The predicate captures a snapshot of the follow-set predicate and a
    /// snapshot of the approved/blocked sets at the instant of the call,
    /// then closes over them. Fail-closed (D6): with no follow set AND no
    /// approved store wired (test paths), the predicate returns `false`.
    ///
    /// POISONED-LOCK FAIL-CLOSED: if the `approved` mutex is poisoned we CANNOT
    /// read the blocklist. Clearing it (treating both sets as empty) would be
    /// fail-OPEN — a blocked-but-followed peer would silently become trusted,
    /// violating "block is absolute". Instead we set `fail_closed` so the
    /// predicate returns `false` for EVERY pubkey: nobody is trusted while we
    /// cannot prove they are not blocked.
    fn trust_predicate(&self) -> impl Fn(&str) -> bool + '_ {
        // Capture follow-set predicate (itself a closure over an Arc<RwLock>).
        let follow_pred = self.follow_set.as_ref().map(|fs| fs.predicate());

        // Snapshot approved/blocked sets from the mutex. A poisoned lock means
        // we cannot read the blocklist → fail CLOSED (trust nobody) rather than
        // dropping blocks.
        let mut fail_closed = false;
        let (approved_snap, blocked_snap) = if let Some(ref arc) = self.approved {
            if let Ok(store) = arc.lock() {
                (store.approved.clone(), store.blocked.clone())
            } else {
                fail_closed = true;
                (Default::default(), Default::default())
            }
        } else {
            (Default::default(), Default::default())
        };

        move |pubkey: &str| {
            // Poisoned approved-store lock: cannot prove the peer is not
            // blocked → deny everyone (block stays absolute).
            if fail_closed {
                return false;
            }
            // Block is an absolute override.
            if blocked_snap.contains(pubkey) {
                return false;
            }
            let followed = follow_pred.as_ref().map(|p| p(pubkey)).unwrap_or(false);
            let approved = approved_snap.contains(pubkey);
            followed || approved
        }
    }

    // ── Approve / block mutating methods ─────────────────────────────────

    /// Approve `pubkey_hex` — clears any block. Caller MUST persist the store
    /// to disk and call `self.infra.bump()` after this returns.
    pub fn approve_peer(&self, pubkey_hex: &str) {
        if let Some(ref arc) = self.approved {
            if let Ok(mut store) = arc.lock() {
                store.approve(pubkey_hex);
            }
        }
    }

    /// Block `pubkey_hex` — clears any approval. Caller MUST persist the store
    /// to disk and call `self.infra.bump()` after this returns.
    pub fn block_peer(&self, pubkey_hex: &str) {
        if let Some(ref arc) = self.approved {
            if let Ok(mut store) = arc.lock() {
                store.block(pubkey_hex);
            }
        }
    }

    /// Remove an explicit approval for `pubkey_hex`. Caller MUST persist and bump.
    pub fn remove_peer_approval(&self, pubkey_hex: &str) {
        if let Some(ref arc) = self.approved {
            if let Ok(mut store) = arc.lock() {
                store.remove_approval(pubkey_hex);
            }
        }
    }

    /// Remove an explicit block for `pubkey_hex`. Caller MUST persist and bump.
    pub fn remove_peer_block(&self, pubkey_hex: &str) {
        if let Some(ref arc) = self.approved {
            if let Ok(mut store) = arc.lock() {
                store.remove_block(pubkey_hex);
            }
        }
    }

    /// Test constructor.
    #[cfg(test)]
    pub fn for_test() -> Self {
        Self::new(Infra::for_test())
    }

    /// Test constructor with a shared approved-peer store for trust-predicate
    /// tests that exercise the approve/block path.
    #[cfg(test)]
    pub fn for_test_with_approved(store: Arc<Mutex<ApprovedPeerStore>>) -> Self {
        Self::new(Infra::for_test()).with_approved_peers(store)
    }

    /// Seed the outbound-turn slot from a persisted cache loaded at init.
    ///
    /// Called once from `register.rs` after loading the on-disk cache so the
    /// very first projection already includes turns from prior sessions.
    pub fn seed_outbound_turns(&self, turns: Vec<OutboundTurn>) {
        if let Ok(mut slot) = self.outbound_turns.lock() {
            *slot = turns;
        }
    }

    // ── Account switch ────────────────────────────────────────────────────

    /// Clear all per-account social state — the follow-list snapshot, the
    /// cached agent notes, and the outbound turns.  Called from the
    /// identity-change hook in `register.rs` so no cross-account state
    /// survives an A→B switch.
    pub fn clear_for_account_switch(&self) {
        if let Ok(mut s) = self.social_slot.lock() {
            *s = None;
        }
        if let Ok(mut n) = self.agent_notes.lock() {
            n.clear();
        }
        if let Ok(mut o) = self.outbound_turns.lock() {
            o.clear();
        }
    }

    /// Append an outbound turn to the in-memory slot.
    ///
    /// Deduplication (by `event_id`) is the caller's responsibility — the
    /// auto-responder already gates on the responder cache before publishing,
    /// so duplicates should never arrive. This method simply appends without
    /// an additional dedup check to avoid O(N) scans on the hot responder path.
    ///
    /// The caller is responsible for persisting the corresponding
    /// `OutboundTurnCache` to disk. This method ONLY mutates the in-memory
    /// projection slot.
    pub fn record_outbound_turn(&self, turn: OutboundTurn) {
        if let Ok(mut slot) = self.outbound_turns.lock() {
            slot.push(turn);
        }
    }

    // ── Snapshot projections ──────────────────────────────────────────────

    /// Clone the current social snapshot for projection.
    pub fn social_snapshot(&self) -> Option<SocialSnapshot> {
        self.social_slot.lock().ok().and_then(|s| s.clone())
    }

    /// Project the cached agent notes into wire DTOs, computing `trusted`
    /// **live** against the composed trust predicate at build time.
    ///
    /// `trust(pubkey) = (followed(pubkey) || approved(pubkey)) && !blocked(pubkey)`
    ///
    /// A follow/unfollow, approve, or block flips the verdict on ALL existing
    /// notes immediately with no stale freeze. Fail-closed: with no follow set
    /// AND no approved store wired (tests), the predicate returns `false` (D6).
    pub fn agent_notes_snapshot(&self) -> Vec<AgentNoteSummary> {
        let cached = match self.agent_notes.lock() {
            Ok(n) => n.clone(),
            Err(_) => return Vec::new(),
        };
        // Build the composed predicate ONCE per projection.
        let predicate = self.trust_predicate();
        cached
            .into_iter()
            .map(|note| {
                let trusted = predicate(&note.author_hex);
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

    /// Project inbound agent notes + outbound turns into NIP-10-threaded
    /// [`NostrConversationDTO`]s for the `podcast.social` push sidecar.
    ///
    /// ## Grouping algorithm
    ///
    /// Each inbound [`CachedAgentNote`] with a `root_event_id` is bucketed
    /// under that root. Notes without a root are themselves the root (they open
    /// a new thread — `root_event_id = note.id`). Outbound turns carry an
    /// explicit `root_event_id` set by the auto-responder.
    ///
    /// Both sides are merged into `turns`, sorted ascending by `created_at`.
    ///
    /// ## Trust
    ///
    /// The `trusted` field on each conversation is computed live (same
    /// `ActiveFollowSet` predicate as `agent_notes_snapshot`), keyed on the
    /// primary counterparty's hex. Fail-closed (D6).
    pub fn nostr_conversations_snapshot(&self) -> Vec<NostrConversationDTO> {
        let notes = match self.agent_notes.lock() {
            Ok(n) => n.clone(),
            Err(_) => return Vec::new(),
        };
        let outbound = match self.outbound_turns.lock() {
            Ok(o) => o.clone(),
            Err(_) => return Vec::new(),
        };

        // Build composed trust predicate once per projection.
        let predicate = self.trust_predicate();

        // Keyed by root_event_id. Value: (counterparty_hex, participants, turns).
        let mut threads: HashMap<String, (String, Vec<String>, Vec<NostrConversationTurnDTO>)> =
            HashMap::new();

        // ── Fold inbound notes ────────────────────────────────────────────────
        for note in &notes {
            let root = note
                .root_event_id
                .clone()
                .unwrap_or_else(|| note.id.clone());
            let entry = threads
                .entry(root)
                .or_insert_with(|| (note.author_hex.clone(), Vec::new(), Vec::new()));
            // Participants: accumulate unique hex pubkeys.
            if !entry.1.contains(&note.author_hex) {
                entry.1.push(note.author_hex.clone());
            }
            entry.2.push(NostrConversationTurnDTO {
                event_id: note.id.clone(),
                direction: "inbound".to_string(),
                pubkey_hex: note.author_hex.clone(),
                created_at: note.created_at,
                content: note.content.clone(),
            });
        }

        // ── Fold outbound turns ───────────────────────────────────────────────
        for turn in &outbound {
            let entry = threads
                .entry(turn.root_event_id.clone())
                .or_insert_with(|| (turn.counterparty_hex.clone(), Vec::new(), Vec::new()));
            // Add the counterparty to participants if missing.
            if !entry.1.contains(&turn.counterparty_hex) {
                entry.1.push(turn.counterparty_hex.clone());
            }
            entry.2.push(NostrConversationTurnDTO {
                event_id: turn.event_id.clone(),
                direction: "outbound".to_string(),
                pubkey_hex: turn.counterparty_hex.clone(), // our reply targets them
                created_at: turn.created_at,
                content: turn.content.clone(),
            });
        }

        // ── Assemble DTOs ─────────────────────────────────────────────────────
        let mut conversations: Vec<NostrConversationDTO> = threads
            .into_iter()
            .map(|(root_event_id, (counterparty_hex, participants, mut turns))| {
                // Sort turns chronologically.
                turns.sort_by_key(|t| t.created_at);
                let first_seen = turns.first().map(|t| t.created_at).unwrap_or(0);
                let last_activity = turns.last().map(|t| t.created_at).unwrap_or(0);
                let trusted = predicate(&counterparty_hex);
                NostrConversationDTO {
                    root_event_id,
                    counterparty_hex,
                    participants,
                    turns,
                    trusted,
                    first_seen,
                    last_activity,
                }
            })
            .collect();

        // Sort conversations newest-first by last_activity.
        conversations.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));
        conversations
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_note_handler::CachedAgentNote;
    use crate::ffi::projections::SocialSnapshot;
    use crate::store::outbound_turn_cache::OutboundTurn;
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

    // ── Composed trust predicate truth table ─────────────────────────────────

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
        let before = state.agent_notes_snapshot();
        assert!(!before[0].trusted, "must be untrusted before approve");

        // Approve via mutating helper.
        state.approve_peer(PEER);

        // After approve: trusted.
        let after = state.agent_notes_snapshot();
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
        let before = state.agent_notes_snapshot();
        assert!(before[0].trusted, "must be trusted before block");

        // Block via mutating helper.
        state.block_peer(PEER);

        // After block: untrusted despite follow.
        let after = state.agent_notes_snapshot();
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
}
