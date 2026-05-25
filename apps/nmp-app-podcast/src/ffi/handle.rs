//! Opaque handle returned by `nmp_app_podcast_register` and consumed by
//! `nmp_app_podcast_snapshot` / `nmp_app_podcast_unregister`.

use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicU64;

use nmp_ffi::NmpApp;

use crate::ffi::projections::{BriefingSnapshot, NostrShowSummary, PodcastSummary};
use crate::ffi::projections::{PodcastSummary, WikiArticle};
use crate::player::PlayerActor;
use crate::queue::PlaybackQueue;
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
    /// Active briefing projection. M9.A stub: written by
    /// `briefings_handler::handle_generate_briefing` to flip
    /// `is_generating = true` so the iOS Briefings tab sees the
    /// composer is in flight. Full lifecycle (segments, last_generated_at)
    /// lands in M9.B when the composer + scheduler wire up.
    pub(super) briefing: Arc<Mutex<Option<BriefingSnapshot>>>,
    /// Playback "Up Next" queue. Mutated by the queue action handler on the
    /// actor thread; read by the snapshot projection on the main thread.
    pub(super) queue: Arc<Mutex<PlaybackQueue>>,
    /// All AI-wiki articles the user has generated. Written by the
    /// `podcast.wiki.{generate,delete}` ops on the actor thread; read by
    /// `build_snapshot_payload` on the main thread.
    pub(super) wiki_articles: Arc<Mutex<Vec<WikiArticle>>>,
    /// Transient result of the most recent `podcast.wiki.search`. Written
    /// by the search op; cleared by a subsequent search that returns
    /// nothing (or by `podcast.wiki.delete` of a referenced article — the
    /// scaffold only mutates `wiki_articles` so search results may go
    /// stale; that's tracked as a follow-up alongside real LLM synthesis).
    pub(super) wiki_search_results: Arc<Mutex<Vec<WikiArticle>>>,
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
