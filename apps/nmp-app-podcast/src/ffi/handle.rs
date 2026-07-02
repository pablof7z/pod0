//! Opaque handle returned by `nmp_app_podcast_register` and consumed by
//! `nmp_app_podcast_snapshot` / `nmp_app_podcast_unregister`.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use crate::state::PodcastAppState;
use crate::store::agent_note_responder_cache::ResponderCache;
use crate::store::approved_peer_store::ApprovedPeerStore;
use crate::store::outbound_turn_cache::OutboundTurnCache;

use nmp_native_runtime::NmpApp;
// AtomicU64, Ordering, Runtime, SnapshotUpdateSignal removed in Step N+1 —
// these lived in the mirror fields now deleted.

/// Diagnostic publish state retained per-podcast across snapshot ticks.
///
/// `show_event_json` is the most recently-built unsigned `kind:10154`
/// event JSON (debug surface — relay publishing is still pending the
/// broader Nostr infrastructure). `last_published_at` is Unix seconds.
/// `blossom_pending` maps Blossom correlation IDs to episode IDs for uploads
/// awaiting resolution. `episode_publish_failed` tracks episodes whose most
/// recent publish attempt failed (relay rejection or upload error).
#[derive(Clone, Debug, Default)]
pub struct OwnedPublishState {
    pub show_event_json: Option<String>,
    pub last_published_at: Option<i64>,
    /// correlation_id → episode_id for Blossom uploads in progress.
    pub blossom_pending: HashMap<String, String>,
    /// episode_ids whose most recent publish attempt failed.
    pub episode_publish_failed: HashSet<String>,
}

/// Opaque handle returned by [`super::nmp_app_podcast_register`]. Boxed on the
/// heap so the address is stable; the Swift consumer holds the raw pointer
/// until it calls [`super::nmp_app_podcast_unregister`].
pub struct PodcastHandle {
    pub(super) app: *mut NmpApp,
    /// Composed state root.  Inbox (Step 7), Knowledge (Step 1), Wiki (Step 2),
    /// Picks (Step 3), Categories (Step 4), Clips (Step 5a), Transcripts (Step 5b),
    /// Tasks (Step 6), Comments (Step 8), Discovery (Step 9), Social (Step 10),
    /// AgentChat (Step 11), Voice (Step 12), Publish (Step 13), Playback (Step 14),
    /// Library/identity (Step 15) all live here.
    pub(crate) state: Arc<PodcastAppState>,
    // store removed in Step 15 — now owned by `state.library.store`.
    // identity removed in Step 15 — now owned by `state.library.identity`.
    // rev removed in Step N+1 — now `state.infra.rev`.
    // snapshot_signal removed in Step N+1 — now `state.infra.signal`.
    // runtime removed in Step N+1 — now `state.infra.runtime`.
    // search_results removed in Step 9 — now owned by `state.discovery` (DiscoveryState).
    // nostr_results removed in Step 9 — dead duplicate Arc; observer now shares
    // from `state.discovery.nostr_results` via register.rs.
    /// Auto-responder dedup + turn-count persistence. Seeded in
    /// `nmp_app_podcast_set_data_dir` from the `agent-note-responder-cache.json`
    /// sidecar; written by `agent_note_responder` after each successful publish.
    /// Shared with the `AgentNotesObserver` (via `with_responder`) so the guard
    /// check and the cache update run against the same in-memory state.
    pub(crate) responder_cache: Arc<Mutex<ResponderCache>>,
    /// Outbound-turn disk-persistence cache. Seeded in `nmp_app_podcast_set_data_dir`
    /// from the `outbound-turn-cache.json` sidecar. Written by `agent_note_responder`
    /// after each successful auto-reply publish. Shared with `AgentNotesObserver`
    /// (via `with_responder`) so persisted turns survive app restarts and the
    /// in-memory social state is seeded on boot.
    pub(crate) outbound_turn_cache: Arc<Mutex<OutboundTurnCache>>,
    /// Kernel-owned peer approve/block allow-list. Seeded from disk in
    /// `nmp_app_podcast_set_data_dir`; the same Arc is injected into
    /// `state.social` so the trust predicate reads it live. Held here so
    /// `data_dir.rs` can seed it after the data dir is bound.
    pub(crate) approved_peer_store: Arc<Mutex<ApprovedPeerStore>>,
    /// Rust-owned policy state for the agent `ask` tool. Swift presents the
    /// current row and executes owner actions; FIFO/current promotion, timeout
    /// labels, and result envelopes live in `ffi::agent_ask`.
    pub(crate) ask_state: Arc<Mutex<super::agent_ask::AgentAskState>>,
    /// Host callback used only to notify Swift when Rust-owned ask lifecycle
    /// events complete asynchronously, currently timeout expiry.
    pub(crate) ask_callback: Arc<Mutex<super::agent_ask::AgentAskCallbackState>>,
    /// Rev-keyed snapshot cache. `build_snapshot_payload` writes `(rev, json)`
    /// here after every rebuild; a later same-`rev` request returns the cached
    /// string without re-serializing the entire library.
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
    // feedback removed in Step 16 — now owned by state.feedback (FeedbackRuntime).
    // feed_fetch removed in Step 16 — now owned by state.feed_fetch (FeedFetchCoordinator).
    // runtime removed in Step N+1 — now state.infra.runtime.
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
    /// Bump the snapshot rev. Step N+1: delegates to `state.infra.bump()`
    /// which owns the canonical signal + rev fallback logic.
    ///
    /// perf/domain-rev-wiring: the cross-thread report FFIs (`audio_report`,
    /// `download_report`) call the domain-scoped variants below so the right
    /// per-domain push delta fires. This bare form (Misc-scoped root infra) is
    /// kept for any caller without a specific domain.
    pub(crate) fn bump_snapshot_rev(&self) {
        self.state.infra.bump();
    }

    /// Conditional bump — delegates to `state.infra.bump_if(changed)`.
    pub(crate) fn bump_snapshot_rev_if(&self, changed: bool) {
        self.state.infra.bump_if(changed);
    }

    /// Bump the global rev AND a specific push domain's rev (signal-aware).
    ///
    /// Cross-thread report paths use this so a playback writeback fires the
    /// `podcast.playback` delta and a download completion fires `podcast.library`.
    pub(crate) fn bump_snapshot_rev_domain(&self, domain: crate::state::Domain) {
        self.state.infra.bump_domain_explicit(domain);
    }

    /// Conditional domain-scoped bump — no-op when `changed` is false.
    pub(crate) fn bump_snapshot_rev_domain_if(&self, domain: crate::state::Domain, changed: bool) {
        if changed {
            self.state.infra.bump_domain_explicit(domain);
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

    // ── Headless-only test surface ────────────────────────────────────────────
    //
    // These methods are compiled only when the `headless` feature is active
    // (the headless scenario binary).  They expose internal state mutation
    // points that would be inappropriate for production code but are required
    // to drive account-switch and leak-guard scenarios without a live relay.

    /// Inject a synthetic [`crate::ffi::projections::SocialSnapshot`] into the
    /// social slot — simulating a kind:3 push frame from `FollowListObserver`.
    ///
    /// Used by the `account_switch` headless scenario to pre-populate social
    /// state for account A before triggering the identity-change hook.
    #[cfg(feature = "headless")]
    pub fn headless_inject_social_snapshot(
        &self,
        snap: crate::ffi::projections::SocialSnapshot,
    ) {
        if let Ok(mut slot) = self.state.social.social_slot.lock() {
            *slot = Some(snap);
        }
    }

    /// Inject a synthetic [`crate::agent_note_handler::CachedAgentNote`] into
    /// the agent-notes cache — simulating an inbound kind:1 from `AgentNotesObserver`.
    ///
    /// Used by the `account_switch` headless scenario to pre-populate agent
    /// notes for account A before triggering the identity-change hook.
    #[cfg(feature = "headless")]
    pub fn headless_inject_agent_note(
        &self,
        note: crate::agent_note_handler::CachedAgentNote,
    ) {
        if let Ok(mut notes) = self.state.social.agent_notes.lock() {
            notes.push(note);
        }
    }

    /// Read the current social snapshot slot (for post-switch leak assertions).
    ///
    /// Returns `None` if the slot is empty (cleared by `clear_for_account_switch`)
    /// or if the mutex is poisoned.
    #[cfg(feature = "headless")]
    pub fn headless_social_snapshot(
        &self,
    ) -> Option<crate::ffi::projections::SocialSnapshot> {
        self.state.social.social_slot.lock().ok().and_then(|s| s.clone())
    }

    /// Read the current agent-notes cache length (for post-switch leak assertions).
    ///
    /// Returns `0` if the cache is empty (cleared by `clear_for_account_switch`)
    /// or if the mutex is poisoned.
    #[cfg(feature = "headless")]
    pub fn headless_agent_notes_len(&self) -> usize {
        self.state
            .social
            .agent_notes
            .lock()
            .map(|n| n.len())
            .unwrap_or(0)
    }

    /// Drive `SocialState::clear_for_account_switch` directly — used by the
    /// headless `account_switch` scenario to test the leak-guard path without
    /// requiring a live NMP kernel signer switch (which would need a relay
    /// sign-in flow not supported in the headless harness).
    ///
    /// This invokes the exact same `clear_for_account_switch` code path that
    /// `register_identity_change_observer` fires in production.  Assertions
    /// immediately after this call validate the slot-clearing contract.
    #[cfg(feature = "headless")]
    pub fn headless_trigger_account_switch_clear(&self) {
        self.state.social.clear_for_account_switch();
    }
}
