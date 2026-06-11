//! Composed state tree for the podcast app.
//!
//! This module is the heart of the god-root consolidation (design doc at
//! `docs/design/podcast-app-state-refactor.md`).  It replaces the
//! field-for-field mirror between `PodcastHandle` and
//! `PodcastHostOpHandler` with a single `Arc<PodcastAppState>` shared by
//! both seams.
//!
//! ## Migration status
//!
//! Step 0: scaffolding — `Slot<T,D>`, `Durability`, `Infra`, and an *empty*
//! `PodcastAppState` (holds only `infra`).  Both god-structs still own their
//! old fields; the new `state` field is added alongside but unused.
//!
//! Step 1: Knowledge substate — `KnowledgeState` owns the two knowledge
//! `Arc`s, which are removed from both god-structs in the same PR.
//!
//! Steps 2-N are defined in the design doc.

pub mod knowledge;
pub mod slot;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::runtime::Runtime;

use crate::snapshot_signal::SnapshotUpdateSignal;

pub use slot::{Derived, Durability, Persisted, Session, Slot};

// ── Infra ─────────────────────────────────────────────────────────────────────

/// Cross-cutting infrastructure every substate needs to bump the snapshot and
/// spawn off-actor work.
///
/// `Infra` is cheap to clone (three `Arc` clones) and is injected into every
/// substate at construction so substate methods need no extra parameters to
/// bump `rev`.
///
/// ## Rev-bump discipline
///
/// Call `infra.bump()` **after** releasing all `Slot` guards (never hold a
/// guard across the bump — `bump()` posts on the actor channel, which can
/// cause priority inversion with the actor thread).
#[derive(Clone)]
pub struct Infra {
    pub rev: Arc<AtomicU64>,
    pub(crate) signal: Option<SnapshotUpdateSignal>,
    pub runtime: Arc<Runtime>,
}

impl Infra {
    /// Bump the snapshot rev.
    ///
    /// When a `SnapshotUpdateSignal` is wired (production), this posts
    /// `MarkChangedSinceEmit` on the actor channel, which coalesces multiple
    /// in-flight bumps into one rev increment.  Without a signal (tests) it
    /// falls back to a raw `fetch_add(1, Relaxed)`.
    ///
    /// Replaces the `match self.snapshot_signal { Some(s)=>s.bump(), … }`
    /// pattern repeated in ~12 handlers.
    pub fn bump(&self) {
        match &self.signal {
            Some(s) => s.bump(),
            None => {
                self.rev.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Bump only when `changed` is true — avoids a no-op bump that would
    /// cause a snapshot rebuild for nothing.
    pub fn bump_if(&self, changed: bool) {
        if changed {
            self.bump();
        }
    }

    /// Read the current rev (for test assertions).
    #[cfg(test)]
    pub fn rev(&self) -> u64 {
        self.rev.load(Ordering::Relaxed)
    }

    /// Construct a minimal `Infra` suitable for unit tests (no signal, bare
    /// single-thread runtime so test code doesn't need a full multi-thread
    /// scheduler).
    #[cfg(test)]
    pub fn for_test() -> Self {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test tokio runtime");
        Self {
            rev: Arc::new(AtomicU64::new(1)),
            signal: None,
            runtime: Arc::new(rt),
        }
    }
}

// ── PodcastAppState ───────────────────────────────────────────────────────────

/// The single composed root owning every per-feature substate.
///
/// One `Arc<PodcastAppState>` is shared by the reader seam
/// (`PodcastHandle`) and the writer seam (`PodcastHostOpHandler`).
///
/// ## Migration status
///
/// Step 0: holds only `infra` and `knowledge`.  Remaining substates will be
/// added in Steps 2-N per the design doc.  At each step the corresponding
/// god-struct fields are REMOVED in the same PR (no overlap window).
pub struct PodcastAppState {
    /// Cross-cutting infrastructure (rev + signal + runtime).
    pub infra: Infra,

    /// Knowledge substate (Step 1).
    pub knowledge: knowledge::KnowledgeState,
}

impl PodcastAppState {
    /// Construct the composed state from shared infra and a store clone.
    ///
    /// Each substate seeds its own slots internally and clones `infra`
    /// plus the shared `store` Arc.  The 31-arg positional constructor in
    /// `register.rs` will be replaced by this single call once all steps
    /// are complete.
    pub fn new(
        infra: Infra,
        store: Arc<std::sync::Mutex<crate::store::PodcastStore>>,
    ) -> Self {
        let knowledge = knowledge::KnowledgeState::new(infra.clone(), store);
        Self { infra, knowledge }
    }
}
