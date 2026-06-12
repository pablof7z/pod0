//! Playback substate — Step 14 of the god-root consolidation.
//!
//! Owns the three cross-thread slots previously on both god-structs:
//!
//! * `player`    — `Slot<PlayerActor, Session>`.  Written on the actor thread
//!   (`podcast.player` handler) AND from `nmp_app_podcast_audio_report` on the
//!   platform audio thread.  **Session** durability (positions persist via
//!   `PodcastStore`; the actor itself is transient).
//!
//! * `queue`     — `Slot<PlaybackQueue, Persisted>`.  Written on the actor thread
//!   (queue/player handlers) AND from `nmp_app_podcast_audio_report` auto-advance
//!   (pops the head on `ItemEnd`).  Write-through persisted to
//!   `store.cached_queue` via `PodcastStore::persist_with_queue`.
//!
//! * `downloads` — `Slot<DownloadQueue, Session>`.  Written on the actor thread
//!   (download handlers) AND from `nmp_app_podcast_download_report` on the
//!   platform download thread.  **Session** durability (queue state is rebuilt
//!   from in-flight platform callbacks; completed entries are reflected in the
//!   store's `Episode.downloadState`).
//!
//! ## Cross-thread discipline — THE KEY INVARIANT
//!
//! Report FFIs (`audio_report`, `download_report`) call `.share()` on these
//! slots to get a bare `Arc<Mutex<T>>`, which they already know how to lock.
//! The lock topology is **UNCHANGED** — it is still one `Mutex` per slot.
//! `.share()` only changes where the Arc is sourced from (`state.playback.*`
//! instead of the god-struct field).
//!
//! ## Handler methods
//!
//! Queue mutations from `podcast.player` and `podcast.queue` are forwarded
//! here so the slot is mutated in exactly one place.  Every method follows the
//! lock discipline: release the slot guard BEFORE calling `infra.bump()`.
//!
//! ## File-length ceiling
//!
//! AGENTS.md hard limit is 500 lines.

use std::sync::{Arc, Mutex};

use crate::download::DownloadQueue;
use crate::ffi::actions::queue_module::QueueAction;
use crate::player::PlayerActor;
use crate::queue::PlaybackQueue;
use crate::state::slot::{Persisted, Session};
use crate::state::{Infra, Slot};
use crate::store::PodcastStore;

/// Playback feature substate — player actor, Up-Next queue, download queue.
///
/// Constructed once in `PodcastAppState::new` and referenced as
/// `state.playback` on both seams.  The three `Slot`s replace the
/// identically-named `Arc<Mutex<_>>` fields that were on
/// `PodcastHostOpHandler` and `PodcastHandle`.
pub struct PlaybackState {
    /// Kernel-side audio player state machine.
    ///
    /// **Session** — positions are mirrored into `PodcastStore`; the actor
    /// itself resets on process restart.
    ///
    /// Writers: actor thread (player_actions handler), audio-report FFI thread.
    /// Readers: snapshot path (`build_podcast_update`), audio-report FFI.
    pub player: Slot<PlayerActor, Session>,

    /// "Up Next" playback queue — ordered list of episode IDs.
    ///
    /// **Persisted** — the canonical ordering is written through to
    /// `PodcastStore::persist_with_queue` after every mutation so it
    /// survives a process restart.
    ///
    /// Writers: actor thread (queue/player handlers), audio-report auto-advance.
    /// Readers: snapshot path (`build_podcast_update`).
    pub queue: Slot<PlaybackQueue, Persisted>,

    /// Per-episode download state machine.
    ///
    /// **Session** — terminal states (Completed/Cancelled/Failed) are reflected
    /// into `Episode.downloadState` inside `PodcastStore`; the queue itself
    /// is rebuilt from platform callbacks after a restart.
    ///
    /// Writers: actor thread (download handlers), download-report FFI thread.
    /// Readers: snapshot path (`build_podcast_update`), download-report FFI.
    pub downloads: Slot<DownloadQueue, Session>,

    /// Rev + signal + runtime — for `infra.bump()` after mutations.
    pub(crate) infra: Infra,

    /// Canonical persisted store — queue mutations flush here.
    store: Arc<Mutex<PodcastStore>>,
}

impl PlaybackState {
    /// Construct PlaybackState with empty, default-initialized slots.
    ///
    /// Called from `PodcastAppState::new`; `infra` and `store` are shared
    /// with the other substates.
    pub fn new(infra: Infra, store: Arc<Mutex<PodcastStore>>) -> Self {
        Self {
            player: Slot::new(PlayerActor::new()),
            queue: Slot::new(PlaybackQueue::new()),
            downloads: Slot::new(DownloadQueue::new()),
            infra,
            store,
        }
    }

    // ── Queue mutations (actor-thread-only; called from router) ───────────────

    /// Apply a `podcast.queue` action to the canonical queue, persist the new
    /// ordering, and bump `rev`.  Mirrors the pre-migration `handle_queue_action`
    /// free function, now scoped to this substate.
    pub fn handle_queue_action(&self, action: QueueAction) -> serde_json::Value {
        let items = {
            let mut q = match self.queue.lock() {
                Ok(q) => q,
                Err(_) => return serde_json::json!({"ok": false, "error": "queue poisoned"}),
            };
            match action {
                QueueAction::AddNext { episode_id } => q.add_to_front(&episode_id),
                QueueAction::AddLast { episode_id } => q.add_to_end(&episode_id),
                QueueAction::Remove { episode_id } => q.remove(&episode_id),
                QueueAction::Clear => q.clear(),
            }
            q.items().to_vec()
        }; // guard released
        self.persist_queue_items(&items);
        self.infra.bump();
        serde_json::json!({"ok": true})
    }

    /// Flush the given queue ordering to `PodcastStore::persist_with_queue`.
    /// Does NOT bump rev — callers do that after releasing all guards.
    pub(crate) fn persist_queue_items(&self, items: &[String]) {
        if let Ok(mut s) = self.store.lock() {
            s.persist_with_queue(items);
        }
    }

    /// Clone the current queue item list for snapshot projection.
    pub fn queue_snapshot(&self) -> Vec<String> {
        self.queue.lock().ok().map(|q| q.items().to_vec()).unwrap_or_default()
    }
}
