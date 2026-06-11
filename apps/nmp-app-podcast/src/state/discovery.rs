//! Discovery substate — Step 9 of the god-root consolidation.
//!
//! Owns the two slots that were previously mirrored between
//! `PodcastHandle` and `PodcastHostOpHandler`:
//!
//! * `itunes_results` — `Vec<PodcastSummary>` from iTunes search.  **Session**.
//! * `nostr_results`  — `Vec<NostrShowSummary>` from NIP-F4 discovery.  **Session**.
//!
//! ## Observer wiring (dead-duplicate removal)
//!
//! `NostrDiscoveryObserver` (in `crate::discover_nostr`) writes `nostr_results`
//! off the actor thread.  It obtains its `Arc<Mutex<Vec<NostrShowSummary>>>` via
//! `state.discovery.nostr_results.share()` at registration time in `register.rs`.
//!
//! **Dead-duplicate removal**: the previous `PodcastHostOpHandler.nostr_results`
//! Arc was a dead clone — never read or written by the handler itself; the live
//! write path was always the observer's own Arc from `register.rs`.  Removing
//! the handler field (same PR this substate is added) is the natural outcome:
//! the observer now shares from `state.discovery.nostr_results`, and there is
//! one canonical Arc, not two.
//!
//! ## File-length ceiling
//!
//! AGENTS.md hard limit is 500 lines.

use crate::ffi::projections::{NostrShowSummary, PodcastSummary};
use crate::state::slot::Session;
use crate::state::{Infra, Slot};

/// Discovery feature substate.
///
/// Constructed once in `PodcastAppState::new` and referenced via
/// `state.discovery` on both seams.  All methods are `&self`.
pub struct DiscoveryState {
    /// Transient iTunes search results. Written by `handle_search_itunes` on
    /// the actor thread; read by the snapshot projection.
    /// Session durability — cleared and repopulated on each SearchItunes action.
    pub itunes_results: Slot<Vec<PodcastSummary>, Session>,
    /// Transient NIP-F4 (`kind:10154`) Nostr discovery results. Written by
    /// `NostrDiscoveryObserver` off the actor thread via `.share()`;
    /// read by the snapshot projection.
    pub nostr_results: Slot<Vec<NostrShowSummary>, Session>,
    /// Rev + signal + runtime (cloned from `PodcastAppState::infra`).
    /// Kept for future bump-on-write; suppressed until first use.
    #[allow(dead_code)]
    pub(crate) infra: Infra,
}

impl DiscoveryState {
    /// Production constructor — called from `PodcastAppState::new`.
    pub fn new(infra: Infra) -> Self {
        Self {
            itunes_results: Slot::new(Vec::new()),
            nostr_results: Slot::new(Vec::new()),
            infra,
        }
    }

    /// Test constructor.
    #[cfg(test)]
    pub fn for_test() -> Self {
        Self::new(Infra::for_test())
    }

    // ── Snapshot projections ──────────────────────────────────────────────

    /// Clone the current iTunes search results for the snapshot.
    pub fn itunes_snapshot(&self) -> Vec<PodcastSummary> {
        self.itunes_results
            .lock()
            .ok()
            .map(|r| r.clone())
            .unwrap_or_default()
    }

    /// Clone the current Nostr discovery results for the snapshot.
    pub fn nostr_snapshot(&self) -> Vec<NostrShowSummary> {
        self.nostr_results
            .lock()
            .ok()
            .map(|r| r.clone())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::projections::NostrShowSummary;

    #[test]
    fn itunes_snapshot_empty_on_init() {
        let state = DiscoveryState::for_test();
        assert!(state.itunes_snapshot().is_empty());
    }

    #[test]
    fn nostr_snapshot_empty_on_init() {
        let state = DiscoveryState::for_test();
        assert!(state.nostr_snapshot().is_empty());
    }

    #[test]
    fn nostr_share_is_same_arc() {
        // Verify that writing through a shared Arc is visible via the slot.
        let state = DiscoveryState::for_test();
        let shared = state.nostr_results.share();
        {
            let mut guard = shared.lock().unwrap();
            guard.push(NostrShowSummary {
                event_id: "ev1".into(),
                author_pubkey: "pk1".into(),
                title: "Test".into(),
                description: None,
                feed_url: None,
                artwork_url: None,
                categories: vec![],
            });
        }
        assert_eq!(state.nostr_results.lock().unwrap().len(), 1);
        assert_eq!(state.nostr_snapshot().len(), 1);
    }
}
