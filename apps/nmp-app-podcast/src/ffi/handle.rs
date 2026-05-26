//! Opaque handle returned by `nmp_app_podcast_register` and consumed by
//! `nmp_app_podcast_snapshot` / `nmp_app_podcast_unregister`.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};

use nmp_ffi::NmpApp;

use crate::clip_handler::ClipRecord;
use crate::ffi::projections::{
    AgentMessageSummary, AgentPickSummary, AgentTaskSummary, BriefingSnapshot,
    KnowledgeSearchResult, NostrShowSummary, PodcastSummary, TranscriptEntry, TtsEpisodeSummary,
    VoiceState, WikiArticle,
};
use crate::player::PlayerActor;
use crate::queue::PlaybackQueue;
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
    /// AI agent picks, recomputed heuristically after every successful feed
    /// refresh and on explicit `podcast.picks.refresh` dispatches. Read by
    /// `build_snapshot_payload` on each tick. See `picks_handler` for the
    /// compute path.
    pub(super) picks: Arc<Mutex<Vec<AgentPickSummary>>>,
    /// Agent-scheduled tasks. Mutated by `podcast.tasks.*` action ops
    /// (see `tasks_handler.rs`); read by `build_snapshot_payload`.
    /// Seeded with two defaults in `register.rs` so the iOS UI has
    /// rows to render on first launch.
    pub(super) agent_tasks: Arc<Mutex<Vec<AgentTaskSummary>>>,
    /// Transient RAG / knowledge-search results. Written by
    /// `handle_knowledge_search` on the actor thread; read by
    /// `build_snapshot_payload` on the main thread. Mirrors the
    /// `search_results` shape so the snapshot reads stay symmetric.
    pub(super) knowledge_search_results: Arc<Mutex<Vec<KnowledgeSearchResult>>>,
    /// In-memory list of agent-generated TTS episodes (feature #43).
    /// Written by the `podcast.tts.*` action handlers on the actor thread;
    /// read by `build_snapshot_payload` on the main thread. Not persisted
    /// across kernel lifetimes — disk-backed storage is a follow-up once
    /// the LLM-script generator lands and these become user-visible
    /// artefacts worth keeping.
    pub(super) tts_episodes: Arc<Mutex<Vec<TtsEpisodeSummary>>>,
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
