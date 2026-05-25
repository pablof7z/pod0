//! Opaque handle returned by `nmp_app_podcast_register` and consumed by
//! `nmp_app_podcast_snapshot` / `nmp_app_podcast_unregister`.

use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicU64;

use nmp_ffi::NmpApp;

use crate::ffi::projections::{NostrShowSummary, PodcastSummary};
use crate::player::PlayerActor;
use crate::store::PodcastStore;

/// Opaque handle returned by [`super::nmp_app_podcast_register`]. Boxed on the
/// heap so the address is stable; the Swift consumer holds the raw pointer
/// until it calls [`super::nmp_app_podcast_unregister`].
pub struct PodcastHandle {
    pub(super) app: *mut NmpApp,
    pub(super) player_actor: Arc<Mutex<PlayerActor>>,
    pub(super) store: Arc<Mutex<PodcastStore>>,
    pub(super) rev: Arc<AtomicU64>,
    /// Transient iTunes search results. Written by `handle_search_itunes` on
    /// the actor thread; read by `build_snapshot_payload` on the main thread.
    pub(super) search_results: Arc<Mutex<Vec<PodcastSummary>>>,
    /// Transient NIP-F4 (`kind:10154`) Nostr discovery results. Written by
    /// `handle_discover_nostr` on the actor thread; read by
    /// `build_snapshot_payload` on the main thread.
    pub(super) nostr_results: Arc<Mutex<Vec<NostrShowSummary>>>,
    /// Rev-keyed snapshot cache. `build_snapshot_payload` writes `(rev, json)`
    /// here after every rebuild; the next poll hit with the same `rev` returns
    /// the cached string without re-serializing the entire library.
    pub(super) snapshot_cache: Arc<Mutex<Option<(u64, String)>>>,
}

// SAFETY: the auto-derived `!Send`/`!Sync` comes solely from the
// `app: *mut NmpApp` field. The handle is sound to mark `Send + Sync` because:
//
//   1. Swift owns this handle and only ever touches it from one isolation
//      context. The FFI entry points are reached exclusively from `@MainActor`
//      types, so the handle itself is never raced. (This is a Swift-side caller
//      convention, not a type-system guarantee — documented, not enforced here.)
//   2. The `app` raw pointer is only ever *read* — never mutated from this
//      struct after construction.
//   3. `nmp_app_free` drops `NmpApp`, whose `Drop` sends `Shutdown` and then
//      `join()`s the actor thread before the allocation is freed, fencing any
//      in-flight callbacks.
//
// CALLER CONTRACT: `nmp_app_free` must not be invoked while any kernel
// callback that reaches this handle is still in flight. The in-process
// Rust-trait registration path gets that fence for free (the actor join).
unsafe impl Send for PodcastHandle {}
unsafe impl Sync for PodcastHandle {}
