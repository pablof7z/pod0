//! Opaque handle returned by `nmp_app_podcast_register` and consumed by
//! `nmp_app_podcast_snapshot` / `nmp_app_podcast_unregister`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::state::PodcastAppState;

use nmp_ffi::NmpApp;
use tokio::runtime::Runtime;

use crate::snapshot_signal::SnapshotUpdateSignal;
use crate::store::identity::IdentityStore;
use crate::store::PodcastStore;

/// Diagnostic publish state retained per-podcast across snapshot ticks.
///
/// `show_event_json` is the most recently-built unsigned `kind:10154`
/// event JSON (debug surface — relay publishing is still pending the
/// broader Nostr infrastructure). `last_published_at` is Unix seconds.
#[derive(Clone, Debug, Default)]
pub struct OwnedPublishState {
    pub show_event_json: Option<String>,
    pub last_published_at: Option<i64>,
}

/// Opaque handle returned by [`super::nmp_app_podcast_register`]. Boxed on the
/// heap so the address is stable; the Swift consumer holds the raw pointer
/// until it calls [`super::nmp_app_podcast_unregister`].
pub struct PodcastHandle {
    pub(super) app: *mut NmpApp,
    /// Composed state root.  Inbox (Step 7), Knowledge (Step 1), Wiki (Step 2),
    /// Picks (Step 3), Categories (Step 4), Clips (Step 5a), Transcripts (Step 5b),
    /// Tasks (Step 6), Comments (Step 8), Discovery (Step 9), Social (Step 10),
    /// AgentChat (Step 11), Voice (Step 12), Publish (Step 13),
    /// Playback (Step 14) all live here.
    pub(crate) state: Arc<PodcastAppState>,
    pub(super) store: Arc<Mutex<PodcastStore>>,
    pub(super) identity: Arc<Mutex<IdentityStore>>,
    pub(super) rev: Arc<AtomicU64>,
    pub(crate) snapshot_signal: Option<SnapshotUpdateSignal>,
    // search_results removed in Step 9 — now owned by `state.discovery` (DiscoveryState).
    // nostr_results removed in Step 9 — dead duplicate Arc; observer now shares
    // from `state.discovery.nostr_results` via register.rs.
    /// Rev-keyed snapshot cache. `build_snapshot_payload` writes `(rev, json)`
    /// here after every rebuild; the next poll hit with the same `rev` returns
    /// the cached string without re-serializing the entire library.
    pub(super) snapshot_cache: Arc<Mutex<Option<(u64, String)>>>,
    /// Memoized `strip_html` results keyed by a 64-bit content hash of the raw
    /// RSS description. `build_podcast_update` rebuilds the *entire* library on
    /// every global `rev` bump — and `Playing` position ticks still bump `rev`
    /// ~1 Hz during playback (`ffi/audio_report.rs`), so the full library is
    /// HTML-cleaned once per second on a multi-thousand-episode library, which
    /// dominated the rebuild (see `clean_html`). Descriptions are immutable per
    /// content, so caching the cleaned text turns the per-rebuild cost from
    /// "strip every episode" into "hash + clone every episode". Bounded: cleared
    /// wholesale when it exceeds `CLEAN_HTML_CACHE_CAP` so churned descriptions
    /// (re-sync, feed edits) can't leak unboundedly.
    pub(super) clean_html_cache: Arc<Mutex<HashMap<u64, String>>>,
    // player_actor removed in Step 14 — now owned by `state.playback.player`.
    // queue removed in Step 14 — now owned by `state.playback.queue`.
    // download_queue removed in Step 14 — now owned by `state.playback.downloads`.
    // wiki_articles and wiki_search_results removed in Step 2 —
    // they are now owned by `state.wiki` (WikiState).
    // picks removed in Step 3 — now owned by `state.picks` (PicksState).
    // agent_tasks removed in Step 6 — now owned by `state.tasks` (TasksState).
    // knowledge_search_results and knowledge_store removed in Step 1 —
    // they are now owned by `state.knowledge` (KnowledgeState).
    // clips removed in Step 5a — now owned by `state.clips` (ClipsState).
    // transcripts removed in Step 5b — now owned by `state.transcripts` (TranscriptsState).
    // dismissed_episode_ids removed in Step 7 — now owned by `state.inbox` (InboxState).
    // inbox_triage_cache removed in Step 7 — now owned by `state.inbox` (InboxState).
    // inbox_triage_in_progress removed in Step 7 — now owned by `state.inbox` (InboxState).
    // podcast_keys and publish_state removed in Step 13 —
    // now owned by `state.publish` (PublishState).
    // voice_state and voice_conversation removed in Step 12 —
    // now owned by `state.voice` (VoiceSubstate).
    // conversation, agent_busy, agent_touched removed in Step 11 —
    // now owned by `state.agent_chat` (AgentChatState).
    // categories removed in Step 4 — now owned by `state.categories` (CategoriesState).
    // comments_cache + viewed_comments_episode_id removed in Step 8 —
    // now owned by `state.comments` (CommentsState).
    // social removed in Step 10 — now owned by `state.social` (SocialState).
    // agent_notes removed in Step 10 — dead duplicate Arc; observer now shares
    // from `state.social.agent_notes` via register.rs.
    /// In-app feedback runtime. The app owns only its project coordinate;
    /// `nmp-feedback` owns the relay-pinned interest, publish tags, event cache,
    /// and thread projection. Empty until the first `FetchFeedback` dispatch.
    pub(crate) feedback: nmp_feedback::FeedbackRuntime,
    /// Shared multi-thread Tokio runtime (same `Arc` the host-op handler and
    /// voice manager hold). Kept here for other off-actor work (e.g. wiki,
    /// social). Triage spawning has moved to InboxState (Step 7).
    pub(super) runtime: Arc<Runtime>,
    /// Optimistic-subscribe async feed-fetch coordinator (same `Arc` the
    /// host-op handler holds). The HTTP-report FFI (`nmp_app_podcast_http_report`)
    /// resolves pending feed fetches through this from the platform transport
    /// thread.
    pub(crate) feed_fetch: Arc<crate::feed_fetch::FeedFetchCoordinator>,
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

/// Upper bound on [`PodcastHandle::clean_html_cache`] entries. A library of a
/// few thousand episodes seeds a few thousand stable entries; the cap only
/// trips when description churn (re-sync, repeated feed edits) accumulates
/// stale keys, at which point the cache is cleared and re-warms. Sized well
/// above any realistic working set so steady-state hit rate stays ~100%.
const CLEAN_HTML_CACHE_CAP: usize = 16_384;

impl PodcastHandle {
    pub(crate) fn bump_snapshot_rev(&self) {
        if let Some(signal) = &self.snapshot_signal {
            signal.bump();
        } else {
            self.rev.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(crate) fn bump_snapshot_rev_if(&self, changed: bool) {
        if let Some(signal) = &self.snapshot_signal {
            signal.bump_if(changed);
        } else if changed {
            self.rev.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Memoized [`super::helpers::strip_html`]. The snapshot projection calls
    /// this once per podcast/episode description on every rebuild; since the
    /// raw text is immutable per content, the first call strips-and-caches and
    /// every subsequent rebuild returns a clone of the cleaned string — turning
    /// the hot path from "3-pass HTML strip per episode" into "hash + clone per
    /// episode". This is the contained mitigation for the full-library-rebuild
    /// CPU cost: `build_podcast_update` re-runs on every global `rev` bump, and
    /// `Playing` position ticks still bump `rev` ~1 Hz throughout playback.
    pub(super) fn clean_html(&self, raw: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        raw.hash(&mut hasher);
        let key = hasher.finish();

        if let Ok(cache) = self.clean_html_cache.lock() {
            if let Some(cleaned) = cache.get(&key) {
                return cleaned.clone();
            }
        }

        // Miss: strip outside the lock (the strip is the expensive part; don't
        // hold the cache mutex across it).
        let cleaned = super::helpers::strip_html(raw);

        if let Ok(mut cache) = self.clean_html_cache.lock() {
            if cache.len() >= CLEAN_HTML_CACHE_CAP {
                cache.clear();
            }
            cache.insert(key, cleaned.clone());
        }
        cleaned
    }
}
