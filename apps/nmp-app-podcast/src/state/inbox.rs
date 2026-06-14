//! Inbox substate — Step 7 of the god-root consolidation.
//!
//! Owns the three slots that were previously mirrored between
//! `PodcastHandle` and `PodcastHostOpHandler`:
//!
//! * `dismissed` — `HashSet<String>` of episode ids dismissed this session.
//!   Durability: **Session** (in-memory only; cold launch re-surfaces everything).
//!
//! * `triage_cache` — `HashMap<String, TriageResult>` of LLM triage scores.
//!   Durability: **Session** (the slot is a cache; the file backing is owned by
//!   `crate::store::inbox_triage_cache`; write-through happens inside the
//!   handler via `persist_from_store`).
//!
//! * `triage_in_progress` — `Arc<AtomicBool>` re-entrancy guard.
//!   Durability: **Session** (cleared on process start).
//!
//! The free functions `handle_inbox_action` / `handle_inbox_action_with_signal`
//! in `crate::inbox_handler` are re-exposed as `InboxState::handle` so the
//! router arm calls `self.state.inbox.handle(action)` instead of threading
//! six Arc parameters inline.
//!
//! `maybe_enqueue_triage` is exposed as `InboxState::maybe_enqueue_triage`
//! so the feed-refresh / mutation path can call it — replacing the previous
//! snapshot-path invocation (D8 fix in Commit 2).
//!
//! ## File-length ceiling
//!
//! AGENTS.md hard limit is 500 lines.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use crate::ffi::actions::inbox_module::InboxAction;
use crate::ffi::projections::InboxItem;
use crate::inbox_handler::{
    build_inbox, handle_inbox_action, handle_inbox_action_with_signal,
};
use crate::inbox_llm::{TriageResult, TriageStatus};
use crate::state::slot::Session;
use crate::state::{Infra, Slot};
use crate::store::PodcastStore;

/// Inbox feature substate.
///
/// Constructed once in `PodcastAppState::new` and referenced via
/// `state.inbox` on both seams.
pub struct InboxState {
    /// Episode ids the user has dismissed this session.
    ///
    /// Session durability: cold launch re-surfaces all episodes so the user
    /// can re-triage after a restart.
    pub dismissed: Slot<HashSet<String>, Session>,

    /// LLM triage scores keyed by episode id.
    ///
    /// Session durability on the slot (it is a cache); the file backing is
    /// owned by `crate::store::inbox_triage_cache` and write-through happens
    /// inside the handler via `persist_from_store`.
    pub triage_cache: Slot<HashMap<String, TriageResult>, Session>,

    /// `true` while a background triage task is running.
    ///
    /// Shared with off-actor tokio tasks that write scores back; lives outside
    /// `Slot<_,_>` because it is an `AtomicBool`, not a `Mutex`-guarded value.
    pub triage_in_progress: Arc<AtomicBool>,

    /// Rev + signal + runtime.
    pub(crate) infra: Infra,

    /// The canonical persisted store — needed by `build_inbox` to iterate
    /// subscribed podcasts, and by `maybe_enqueue_triage` to collect episode ids.
    store: Arc<Mutex<PodcastStore>>,
}

impl InboxState {
    /// Production constructor — seeds all slots internally.
    ///
    /// Called from `PodcastAppState::new_with_identity`.  The Arcs are created
    /// here; `register.rs` accesses them via `.share()` if needed by observers
    /// (inbox has no observer, so none are needed in `register.rs`).
    pub fn new(infra: Infra, store: Arc<Mutex<PodcastStore>>) -> Self {
        Self {
            dismissed: Slot::new(HashSet::new()),
            triage_cache: Slot::new(HashMap::new()),
            triage_in_progress: Arc::new(AtomicBool::new(false)),
            infra,
            store,
        }
    }

    /// Test constructor — no pre-seeded Arcs needed.
    #[cfg(test)]
    pub fn for_test() -> Self {
        Self {
            dismissed: Slot::new(HashSet::new()),
            triage_cache: Slot::new(HashMap::new()),
            triage_in_progress: Arc::new(AtomicBool::new(false)),
            infra: Infra::for_test(),
            store: Arc::new(Mutex::new(PodcastStore::new())),
        }
    }

    // ── Snapshot projection ───────────────────────────────────────────────

    /// Build the inbox item list for the current snapshot tick.
    ///
    /// Pure projection — reads `store`, `dismissed`, and `triage_cache`
    /// under their respective short-duration locks, then releases them.
    /// Must NOT spawn any work (D8 constraint on projection builders).
    pub fn project(&self) -> Vec<InboxItem> {
        build_inbox(
            &self.store,
            &self.dismissed.share(),
            &self.triage_cache.share(),
        )
    }

    /// Read the current `triage_in_progress` flag for the snapshot field.
    pub fn triage_in_progress_snapshot(&self) -> bool {
        self.triage_in_progress
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Most recent successful triage attempt, in Unix seconds.
    ///
    /// Pending entries are retry placeholders, not completed triage results,
    /// so they do not drive the user-facing "Triaged ..." status line.
    pub fn last_triaged_at_snapshot(&self) -> Option<i64> {
        self.triage_cache.lock().ok().and_then(|cache| {
            cache
                .values()
                .filter(|result| result.status == TriageStatus::Ready)
                .map(|result| result.attempted_at)
                .max()
        })
    }

    // ── Proactive trigger ─────────────────────────────────────────────────

    /// Proactive triage trigger — call from the feed-refresh path and any
    /// mutation that changes inbox-relevant state.
    ///
    /// MUST NOT be called from `build_snapshot_payload` / projection builders
    /// (D8: projection builders must be pure and side-effect-free).
    ///
    /// Internally guarded by `triage_in_progress`; cheap no-op if a pass is
    /// already running or nothing needs triage.
    pub fn maybe_enqueue_triage(&self) {
        let rev = &self.infra.rev;
        let runtime = &self.infra.runtime;
        let triage_cache = &self.triage_cache.share();
        let in_progress = &self.triage_in_progress;

        if let Some(signal) = self.infra.signal.clone() {
            crate::inbox_handler::maybe_enqueue_triage_with_signal(
                &self.store,
                triage_cache,
                rev,
                runtime,
                in_progress,
                signal,
            );
        } else {
            crate::inbox_handler::maybe_enqueue_triage(
                &self.store,
                triage_cache,
                rev,
                runtime,
                in_progress,
            );
        }
    }

    // ── Action handler ────────────────────────────────────────────────────

    /// Route a single `podcast.inbox.*` action.
    ///
    /// Replaces the inline `handle_inbox_action_with_signal` / `handle_inbox_action`
    /// call in the `"podcast.inbox"` router arm.
    pub fn handle(&self, action: InboxAction) -> serde_json::Value {
        let store = &self.store;
        let dismissed = &self.dismissed.share();
        let rev = &self.infra.rev;
        let triage_cache = &self.triage_cache.share();
        let runtime = &self.infra.runtime;
        let in_progress = &self.triage_in_progress;

        if let Some(signal) = self.infra.signal.clone() {
            handle_inbox_action_with_signal(
                action,
                store,
                dismissed,
                rev,
                triage_cache,
                runtime,
                in_progress,
                signal,
            )
        } else {
            handle_inbox_action(
                action,
                store,
                dismissed,
                rev,
                triage_cache,
                runtime,
                in_progress,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use chrono::Utc;
    use podcast_core::{Episode, Podcast};

    fn fixture_store() -> Arc<Mutex<PodcastStore>> {
        let now = Utc::now().timestamp();
        let mut store = PodcastStore::new();
        let podcast = Podcast::new("Test Show");
        let podcast_id = podcast.id;
        let ep = Episode::new(
            podcast_id,
            "https://example.com/feed.xml",
            "guid-1",
            "Fresh Episode",
            url::Url::parse("https://ex.com/1.mp3").unwrap(),
            Utc.timestamp_opt(now - 3_600, 0).unwrap(),
        );
        store.subscribe(podcast, vec![ep]);
        Arc::new(Mutex::new(store))
    }

    #[test]
    fn project_returns_unlistened_episodes() {
        let state = InboxState {
            dismissed: Slot::new(HashSet::new()),
            triage_cache: Slot::new(HashMap::new()),
            triage_in_progress: Arc::new(AtomicBool::new(false)),
            infra: Infra::for_test(),
            store: fixture_store(),
        };
        let items = state.project();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].episode_title, "Fresh Episode");
    }

    #[test]
    fn handle_dismiss_removes_episode_from_projection() {
        let store = fixture_store();
        let ep_id = {
            let s = store.lock().unwrap();
            let (_, eps) = s.all_podcasts()[0];
            eps[0].id.0.to_string()
        };
        let state = InboxState {
            dismissed: Slot::new(HashSet::new()),
            triage_cache: Slot::new(HashMap::new()),
            triage_in_progress: Arc::new(AtomicBool::new(false)),
            infra: Infra::for_test(),
            store,
        };

        let out = state.handle(InboxAction::Dismiss {
            episode_id: ep_id.clone(),
        });
        assert_eq!(out["ok"], true);

        let items = state.project();
        assert!(items.iter().all(|i| i.episode_id != ep_id));
    }

    #[test]
    fn handle_mark_listened_removes_episode_from_projection() {
        let store = fixture_store();
        let ep_id = {
            let s = store.lock().unwrap();
            let (_, eps) = s.all_podcasts()[0];
            eps[0].id.0.to_string()
        };
        let state = InboxState {
            dismissed: Slot::new(HashSet::new()),
            triage_cache: Slot::new(HashMap::new()),
            triage_in_progress: Arc::new(AtomicBool::new(false)),
            infra: Infra::for_test(),
            store,
        };

        let out = state.handle(InboxAction::MarkListened {
            episode_id: ep_id.clone(),
        });
        assert_eq!(out["ok"], true);

        let items = state.project();
        assert!(items.iter().all(|i| i.episode_id != ep_id));
    }

    #[test]
    fn triage_in_progress_snapshot_starts_false() {
        let state = InboxState::for_test();
        assert!(!state.triage_in_progress_snapshot());
    }

    #[test]
    fn last_triaged_at_snapshot_uses_latest_ready_entry_only() {
        let state = InboxState::for_test();
        {
            let mut cache = state.triage_cache.lock().unwrap();
            cache.insert("pending".into(), TriageResult::pending(2_000));
            cache.insert(
                "old-ready".into(),
                TriageResult::ready(0.5, "old".into(), vec![], 1_000),
            );
            cache.insert(
                "new-ready".into(),
                TriageResult::ready(0.9, "new".into(), vec![], 1_500),
            );
        }

        assert_eq!(state.last_triaged_at_snapshot(), Some(1_500));
    }

    #[test]
    fn last_triaged_at_snapshot_ignores_pending_only_cache() {
        let state = InboxState::for_test();
        state
            .triage_cache
            .lock()
            .unwrap()
            .insert("pending".into(), TriageResult::pending(2_000));

        assert_eq!(state.last_triaged_at_snapshot(), None);
    }

    #[test]
    fn handle_dismiss_bumps_rev() {
        let store = fixture_store();
        let ep_id = {
            let s = store.lock().unwrap();
            let (_, eps) = s.all_podcasts()[0];
            eps[0].id.0.to_string()
        };
        let state = InboxState {
            dismissed: Slot::new(HashSet::new()),
            triage_cache: Slot::new(HashMap::new()),
            triage_in_progress: Arc::new(AtomicBool::new(false)),
            infra: Infra::for_test(),
            store,
        };
        let rev0 = state.infra.rev();

        state.handle(InboxAction::Dismiss {
            episode_id: ep_id,
        });
        assert!(state.infra.rev() > rev0, "dismiss must bump rev");
    }

    /// D8 doctrine guard: `project()` must never spawn a triage task.
    ///
    /// If this test fails the snapshot builder has regressed to spawning
    /// side-effects from the projection path.
    #[test]
    fn project_does_not_set_triage_in_progress() {
        let state = InboxState {
            dismissed: Slot::new(HashSet::new()),
            triage_cache: Slot::new(HashMap::new()),
            triage_in_progress: Arc::new(AtomicBool::new(false)),
            infra: Infra::for_test(),
            store: fixture_store(),
        };
        // project() is the path called from build_snapshot_payload.
        let _items = state.project();
        // Must NOT have claimed a triage pass — that is only triggered from the
        // feed-refresh path via maybe_enqueue_triage().
        assert!(
            !state.triage_in_progress.load(std::sync::atomic::Ordering::Relaxed),
            "project() must not spawn a triage task (D8 pure-projection doctrine)"
        );
    }
}
