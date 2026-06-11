//! Opaque handle returned by `nmp_app_podcast_register` and consumed by
//! `nmp_app_podcast_snapshot` / `nmp_app_podcast_unregister`.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use nmp_ffi::NmpApp;
use tokio::runtime::Runtime;

use crate::clip_handler::ClipRecord;
use crate::download::DownloadQueue;
use crate::ffi::projections::{
    AgentMessageSummary, AgentNoteSummary, AgentPickSummary, AgentTaskSummary, CommentSummary,
    KnowledgeSearchResult, NostrShowSummary, PodcastSummary, SocialSnapshot, TranscriptEntry,
    VoiceState, WikiArticle,
};
use crate::inbox_llm::TriageResult;
use crate::player::PlayerActor;
use crate::queue::PlaybackQueue;
use crate::snapshot_signal::SnapshotUpdateSignal;
use crate::store::identity::IdentityStore;
use crate::store::{PodcastKeyStore, PodcastStore};

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
    pub(super) player_actor: Arc<Mutex<PlayerActor>>,
    pub(super) store: Arc<Mutex<PodcastStore>>,
    pub(super) identity: Arc<Mutex<IdentityStore>>,
    pub(super) rev: Arc<AtomicU64>,
    pub(crate) snapshot_signal: Option<SnapshotUpdateSignal>,
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
    /// Playback "Up Next" queue. Mutated by the queue action handler on the
    /// actor thread; read by the snapshot projection on the main thread.
    pub(super) queue: Arc<Mutex<PlaybackQueue>>,
    /// Per-episode download queue state machine. Written by the download
    /// action handler and the download-report FFI entry point; read by
    /// `build_snapshot_payload` to populate `PodcastUpdate.downloads`.
    pub(super) download_queue: Arc<Mutex<DownloadQueue>>,
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
    /// AI agent picks, recomputed heuristically after every successful feed
    /// refresh and on explicit `podcast.picks.refresh` dispatches. Read by
    /// `build_snapshot_payload` on each tick. See `picks_handler` for the
    /// compute path.
    pub(super) picks: Arc<Mutex<Vec<AgentPickSummary>>>,
    /// Agent-scheduled tasks. Mutated by `podcast.tasks.*` action ops
    /// (see `tasks_handler.rs`); read by `build_snapshot_payload`.
    /// Seeded with defaults in `register.rs` so the iOS UI has rows to render
    /// on first launch.
    pub(super) agent_tasks: Arc<Mutex<Vec<AgentTaskSummary>>>,
    /// Transient RAG / knowledge-search results. Written by
    /// `handle_knowledge_search` on the actor thread; read by
    /// `build_snapshot_payload` on the main thread. Mirrors the
    /// `search_results` shape so the snapshot reads stay symmetric.
    pub(super) knowledge_search_results: Arc<Mutex<Vec<KnowledgeSearchResult>>>,
    /// In-memory RAG chunk store (M5.3). `KnowledgeAction::IndexEpisode`
    /// chunks the Rust-stored transcript into here; `Search` substring-
    /// matches over the chunks. The handle owns its share of the `Arc` so
    /// the chunk store outlives any single host-op dispatch and a future
    /// snapshot projection can read it; today only the handler writes/reads
    /// it, hence the `allow(dead_code)` on the handle-side owner.
    #[allow(dead_code)]
    pub(super) knowledge_store: Arc<Mutex<podcast_knowledge::KnowledgeStore>>,
    /// User-saved audio clips. Written by `ClipHandler` on the actor
    /// thread; read by `build_snapshot_payload` on the main thread.
    /// In-memory only — clips evaporate on app restart (persistence is
    /// a follow-up).
    pub(crate) clips: Arc<Mutex<Vec<ClipRecord>>>,
    /// Parsed transcript entries keyed by the string form of `EpisodeId`.
    ///
    /// Lives on the handle (not the persisted `PodcastStore`) because
    /// transcripts are per-session, lazy-fetched state — re-fetching on the
    /// next launch is a cheap network hit and avoids growing
    /// `podcasts.json`. Written by `handle_fetch_transcript` on the actor
    /// thread after parsing publisher bytes; read by
    /// `build_snapshot_payload` on every snapshot tick.
    pub(super) transcripts: Arc<Mutex<HashMap<String, Vec<TranscriptEntry>>>>,
    /// Set of episode ids the user has dismissed from the inbox. In-memory
    /// only — the dismissal is a current-session-only signal; cold launch
    /// re-surfaces everything so the user can re-triage. Written by the
    /// inbox handler's `Dismiss` op; read by the inbox projection builder.
    pub(super) dismissed_episode_ids: Arc<Mutex<HashSet<String>>>,
    /// Per-podcast Nostr keypairs for NIP-F4 owned podcasts (features
    /// #27/#28). Written by `podcast.publish.create_owned_podcast` and
    /// cleared by `remove_owned_podcast`; read by every other publish op.
    pub(super) podcast_keys: Arc<Mutex<PodcastKeyStore>>,
    /// Diagnostic publish state per podcast (last show event JSON +
    /// last-published timestamp). Surfaced via `OwnedPodcastInfo` so the
    /// iOS shell can render "last published at …" without a separate
    /// FFI accessor. Keyed by `podcast_id` UUID string (matching the
    /// FFI projection).
    pub(super) publish_state: Arc<Mutex<HashMap<String, OwnedPublishState>>>,
    /// Voice-mode projection state. Mutated by the `podcast.voice.*`
    /// action handler (when the kernel dispatches `VoiceCommand` to the
    /// iOS executor) and by `nmp_app_podcast_voice_report` (when iOS
    /// reports translate back into projection updates). Read by the
    /// snapshot builder on each tick.
    pub(super) voice_state: Arc<Mutex<VoiceState>>,
    /// Voice-mode conversation manager (M5.6-voice). Owns the rolling
    /// STT→LLM→TTS turn history and dispatches LLM replies back to the
    /// iOS voice executor. Invoked from `nmp_app_podcast_voice_report`
    /// when a `VoiceReport::TranscriptFinal` arrives (the user finished
    /// speaking).
    pub(super) voice_conversation: crate::voice_conversation::VoiceConversationManager,
    /// Active agent-chat transcript. Written by the
    /// [`super::actions::agent_module::AgentActionModule`] handler on the
    /// actor thread; read by `build_snapshot_payload` on the main thread.
    /// In-memory only — feature #32 is a UI scaffold, real LLM integration
    /// (and persistence) lands in a follow-up.
    pub(super) conversation: Arc<Mutex<Vec<AgentMessageSummary>>>,
    /// `true` while the kernel is composing an assistant reply (mirrored
    /// into `AgentSnapshot::is_busy`). Stays `false` in the scaffold since
    /// the canned reply is committed synchronously; the flag exists now so
    /// the snapshot reader doesn't need rewiring once streaming lands.
    pub(super) agent_busy: Arc<AtomicBool>,
    /// `true` once the user has interacted with the agent in this kernel
    /// lifetime (Send → flips to `true`, Clear keeps it `true`). Used by
    /// the snapshot builder to keep `agent` `Some` after a clear so the UI
    /// can tell "cleared" from "never touched". Reset only by a process
    /// restart.
    pub(super) agent_touched: Arc<AtomicBool>,
    /// Heuristic categorizer cache: `episode_id -> Vec<category labels>`.
    /// Written by [`crate::host_op_handler::PodcastHostOpHandler`] on the
    /// actor thread (`podcast.categorize.run` /
    /// `podcast.categorize.categorize_episode`, plus the auto-trigger at
    /// the end of every successful feed refresh). Read by
    /// `build_snapshot_payload` to populate
    /// `EpisodeSummary.ai_categories` + the `categories` aggregate.
    pub(super) categories: Arc<Mutex<HashMap<String, Vec<String>>>>,
    /// LLM triage cache: `episode_id -> TriageResult`.
    ///
    /// Populated by `InboxAction::Triage` on the actor thread (running LLM
    /// classification for each unlistened episode). Read by `build_inbox`
    /// to overlay LLM scores and categories over the recency-bucket fallback.
    /// In-memory only — results are recomputed on each explicit Triage action.
    pub(super) inbox_triage_cache: Arc<Mutex<HashMap<String, TriageResult>>>,
    /// `true` while the background LLM triage task is running. Set before
    /// `tokio::spawn` and cleared when the task completes (or errors out).
    /// Surfaced on `PodcastUpdate.inbox_triage_in_progress` so the iOS UI
    /// can show a spinner on the Inbox tab.
    pub(super) inbox_triage_in_progress: Arc<AtomicBool>,
    /// NIP-22 (kind 1111) comment cache, keyed by episode_id string.
    /// Written by `handle_fetch_comments` / `handle_post_comment` on the
    /// actor thread; read by `build_snapshot_payload` on the main thread.
    pub(crate) comments_cache: Arc<Mutex<HashMap<String, Vec<CommentSummary>>>>,
    /// Episode id whose comments the user is currently viewing. Set by
    /// `handle_fetch_comments` (the comments section opens for an episode);
    /// read by `build_snapshot_payload` to project that episode's comments
    /// instead of being limited to the now-playing episode. `None` until the
    /// first `FetchComments` dispatch, in which case the snapshot falls back
    /// to the now-playing episode id.
    pub(crate) viewed_comments_episode_id: Arc<Mutex<Option<String>>>,
    /// NIP-02 social graph snapshot. `None` until the first
    /// `FetchContacts` dispatch completes. Written by
    /// `social_handler::handle_fetch_contacts` on the actor thread; read
    /// by `build_snapshot_payload` on each tick.
    pub(super) social: Arc<Mutex<Option<SocialSnapshot>>>,
    /// Feature #44 — inbound agent-to-agent kind:1 notes addressed to the
    /// active account. `None`/empty until the first `FetchAgentNotes`
    /// dispatch. Written by `agent_note_handler::handle_fetch_agent_notes`
    /// on the actor thread; read by `build_snapshot_payload` on each tick
    /// and projected onto `PodcastUpdate.agent_notes` (reactive push seam).
    pub(crate) agent_notes: Arc<Mutex<Vec<AgentNoteSummary>>>,
    /// In-app feedback runtime. The app owns only its project coordinate;
    /// `nmp-feedback` owns the relay-pinned interest, publish tags, event cache,
    /// and thread projection. Empty until the first `FetchFeedback` dispatch.
    pub(crate) feedback: nmp_feedback::FeedbackRuntime,
    /// Shared multi-thread Tokio runtime (same `Arc` the host-op handler and
    /// voice manager hold). The snapshot path needs it so `maybe_enqueue_triage`
    /// can spawn proactive background triage off the actor thread.
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
