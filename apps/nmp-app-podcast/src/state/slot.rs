//! Type-level durability markers and the `Slot<T, D>` wrapper.
//!
//! `Slot<T, D>` wraps `Arc<Mutex<T>>` with a zero-cost `PhantomData<D>` tag
//! that encodes the *durability class* of the slot at compile time — so the
//! "accidentally persisted a session slot" bug is unrepresentable:
//! `persist()` exists ONLY on `Slot<_, Persisted>`.  A reviewer flipping
//! `Session` → `Persisted` is a visible, reviewable type change, not a
//! silently-added `store.persist()` call.
//!
//! Atomics (`AtomicBool` guards, `rev`) stay bare `Arc<AtomicU64/Bool>` —
//! `Slot` is for `Mutex`-guarded state only.
//!
//! **Lock granularity is UNCHANGED.** Each `Slot` is still one `Mutex`;
//! `share()` returns the inner `Arc<Mutex<T>>` for off-actor writers (report
//! threads / tokio tasks / kernel observers) so their lock discipline is
//! byte-for-byte identical to today.

use std::marker::PhantomData;
use std::sync::{Arc, Mutex, MutexGuard};

// ── Durability markers ────────────────────────────────────────────────────────

/// Sealed marker trait implemented only by the three durability tags below.
pub trait Durability: sealed::Sealed {}

mod sealed {
    pub trait Sealed {}
}

/// The slot's value is persisted to disk (owned by `PodcastStore` or its own
/// file).  Only `Slot<_, Persisted>` exposes `persist()`.
pub struct Persisted;
impl sealed::Sealed for Persisted {}
impl Durability for Persisted {}

/// The slot's value evaporates on process restart — an in-memory cache only.
pub struct Session;
impl sealed::Sealed for Session {}
impl Durability for Session {}

/// The slot's value is derived/recomputed from persisted state; it is never
/// written to disk directly and can always be rebuilt.
pub struct Derived;
impl sealed::Sealed for Derived {}
impl Durability for Derived {}

// ── Slot<T, D> ────────────────────────────────────────────────────────────────

/// A single shared state slot tagged with its durability class `D`.
///
/// The inner `Arc<Mutex<T>>` is identical to the bare `Arc<Mutex<T>>` fields
/// that currently live on `PodcastHandle` / `PodcastHostOpHandler`.  The `D`
/// tag adds zero runtime overhead (PhantomData) while enforcing at compile
/// time that only `Persisted` slots can be persisted.
///
/// ## Lock ordering
///
/// Follow the canonical order documented in `state/mod.rs §6.2`.  Never hold
/// a `Slot` guard across `infra.bump()` (which posts on the actor channel) or
/// `runtime.block_on` / `runtime.spawn`.
pub struct Slot<T, D: Durability> {
    inner: Arc<Mutex<T>>,
    _dur: PhantomData<D>,
}

// Manual Clone: `Arc` clone, not a full deep-copy.
impl<T, D: Durability> Clone for Slot<T, D> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            _dur: PhantomData,
        }
    }
}

impl<T, D: Durability> Slot<T, D> {
    /// Wrap an existing value in a new `Slot` (allocates the `Arc<Mutex<T>>`).
    pub fn new(value: T) -> Self {
        Self {
            inner: Arc::new(Mutex::new(value)),
            _dur: PhantomData,
        }
    }

    /// Wrap an **existing** `Arc<Mutex<T>>` in a `Slot`.
    ///
    /// Used when a substate must share the exact same `Arc` with another
    /// component (e.g. `VoiceConversationManager` shares the same
    /// `voice_state` Arc with `VoiceSubstate` so off-actor writes from the
    /// manager are visible to the snapshot reader through the slot).
    pub fn from_arc(arc: Arc<Mutex<T>>) -> Self {
        Self {
            inner: arc,
            _dur: PhantomData,
        }
    }

    /// Lock the slot.  Named `lock` (not `read`/`write`) because it is the
    /// same `Mutex` — callers supply intent through comments.
    pub fn lock(&self) -> std::sync::LockResult<MutexGuard<'_, T>> {
        self.inner.lock()
    }

    /// Clone the inner `Arc<Mutex<T>>` for an off-actor writer (report thread,
    /// tokio task, kernel observer).  Their lock discipline is unchanged — they
    /// call `.lock()` on the returned `Arc` exactly as before.
    pub fn share(&self) -> Arc<Mutex<T>> {
        self.inner.clone()
    }
}

// Persistence is ONLY available on Persisted slots — compile-time enforcement.
// The `PersistTo` bound is intentionally not yet defined (Step 0 scaffolding);
// this impl block is the hook; each persisted substate will add the bound when
// it wires real persistence.
//
// For now the body is a stub; it will be filled in as substates are migrated.
impl<T> Slot<T, Persisted> {
    /// Placeholder: persistence implementation will be added per-substate as
    /// migration steps land.  This method signature enforces the type-level
    /// invariant (only `Persisted` slots can call it).
    #[allow(dead_code)]
    pub fn mark_persisted(&self) {
        // Body filled in per substate migration step.
    }
}
