//! The `pub extern "C"` registration entry point Swift links against to wire
//! Podcast projections and action namespaces into an [`NmpApp`].

use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicU64;

use nmp_ffi::NmpApp;

use super::actions::chapters_module::ChaptersActionModule;
use super::actions::player_module::PlayerActionModule;
use super::actions::podcast_module::PodcastActionModule;
use super::actions::queue_module::QueueActionModule;
use super::actions::wiki_module::WikiActionModule;
use super::handle::PodcastHandle;
use crate::host_op_handler::PodcastHostOpHandler;
use crate::player::PlayerActor;
use crate::queue::PlaybackQueue;
use crate::store::PodcastStore;

/// Register Podcast projections and action namespaces against `app`. Returns a
/// non-null `*mut PodcastHandle` on success; `null` on any failure (null
/// pointer arguments, slot lock poisoning).
///
/// `app` MUST outlive the returned handle. Call
/// [`nmp_app_podcast_unregister`] before `nmp_app_free`.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_register(
    app: *mut NmpApp,
) -> *mut PodcastHandle {
    if app.is_null() {
        return std::ptr::null_mut();
    }

    // Wire the canonical NMP composition — NIP-02 / NIP-17 / NIP-57 / NIP-65
    // action modules, the kind:10050 ingest parser, the production routing
    // substrate, and the DM-inbox + zap-receipts runtime controllers.
    //
    // SAFETY: caller guarantees `app` is a valid pointer from `nmp_app_new`.
    // No other reference aliases it here — the `&*app` borrow further down is
    // taken only after this exclusive borrow is dropped.
    let app_mut = unsafe { &mut *app };
    nmp_app_template::register_defaults(app_mut);

    // Register action modules: "podcast" (subscribe/refresh), "podcast.player"
    // (playback), "podcast.queue" (Up Next list), and "podcast.chapters"
    // (AI chapter compile).
    app_mut.register_action::<PodcastActionModule>();
    app_mut.register_action::<PlayerActionModule>();
    app_mut.register_action::<QueueActionModule>();
    app_mut.register_action::<ChaptersActionModule>();
    // (playback), and "podcast.wiki" (AI wiki scaffold — PR #39).
    app_mut.register_action::<PodcastActionModule>();
    app_mut.register_action::<PlayerActionModule>();
    app_mut.register_action::<WikiActionModule>();

    // Shared state between the handle (snapshot reader) and the handler (writer).
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let player_actor = Arc::new(Mutex::new(PlayerActor::new()));
    let search_results = Arc::new(Mutex::new(Vec::new()));
    let nostr_results = Arc::new(Mutex::new(Vec::new()));
    let briefing = Arc::new(Mutex::new(None));
    let queue = Arc::new(Mutex::new(PlaybackQueue::new()));
    let wiki_articles = Arc::new(Mutex::new(Vec::new()));
    let wiki_search_results = Arc::new(Mutex::new(Vec::new()));
    // Start at 1 so the first snapshot poll always triggers an iOS update
    // (guard is `update.rev > last_seen_rev`; last_seen_rev starts at 0).
    // Subsequent increments happen in PodcastHostOpHandler on store writes.
    let rev = Arc::new(AtomicU64::new(1));

    // Install the host-op handler (requires &self, so take the ref AFTER the
    // &mut borrow above is released by the block end).
    let app_ref = unsafe { &*app };
    app_ref.set_host_op_handler(Arc::new(PodcastHostOpHandler::new(
        app,
        store.clone(),
        player_actor.clone(),
        search_results.clone(),
        nostr_results.clone(),
        briefing.clone(),
        queue.clone(),
        wiki_articles.clone(),
        wiki_search_results.clone(),
        rev.clone(),
    )));

    Box::into_raw(Box::new(PodcastHandle {
        app,
        player_actor,
        store,
        rev,
        search_results,
        nostr_results,
        snapshot_cache: Arc::new(Mutex::new(None)),
        briefing,
        queue,
        wiki_articles,
        wiki_search_results,
    }))
}
