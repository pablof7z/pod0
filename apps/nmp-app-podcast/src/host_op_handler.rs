//! Actor-thread handler for podcast/player host operations.
//!
//! Owns the kernel-side state every `podcast.*` action mutates and routes
//! incoming action JSON to the appropriate per-domain handler module.
//! Kept under the 500-LOC hard ceiling (AGENTS.md) by extracting:
//!
//! * Podcast/refresh dispatch -> `host_op_handler/podcast_actions.rs`
//! * Player-action dispatch   -> `host_op_handler/player_actions.rs`
//! * Settings-action dispatch -> `host_op_handler/settings_actions.rs`
//! * Capability dispatch helpers -> `host_op_handler/dispatch.rs`
//! * Queue-action dispatch    -> `host_op_handler_queue.rs`
//! * Task-action dispatch     -> `host_op_handler/task_actions.rs`
//! * iTunes search helpers    -> `itunes.rs`
//! * `merge_episodes`         -> `host_op_handler_helpers.rs`
//! * Publish-action dispatch  -> `host_op_publish.rs`
//! * Voice-action dispatch    -> `voice_handler.rs`

use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use tokio::runtime::Runtime;

use nmp_core::substrate::HostOpHandler;
use nmp_ffi::NmpApp;

use crate::agent_handler::AgentChatHandler;
use crate::ai_chapters::{handle_compile_chapters, handle_compile_chapters_with_signal};
use crate::categorization::{
    handle_categorize_episode, handle_run as categorization_run,
    handle_run_with_signal as categorization_run_with_signal,
};
use crate::clip_handler::{ClipHandler, ClipRecord};
use crate::download::DownloadQueue;
use crate::feed_fetch::FeedFetchCoordinator;
use crate::ffi::actions::agent_module::AgentChatAction;
use crate::ffi::actions::categorization_module::CategorizationAction;
use crate::ffi::actions::chapters_module::ChaptersAction;
use crate::ffi::actions::clip_module::ClipAction;
use crate::ffi::actions::identity_module::IdentityAction;
use crate::ffi::actions::inbox_module::InboxAction;
use crate::ffi::actions::knowledge_module::KnowledgeAction;
use crate::ffi::actions::memory_module::MemoryAction;
use crate::ffi::actions::picks_module::PicksAction;
use crate::ffi::actions::player_module::PlayerAction;
use crate::ffi::actions::podcast_module::PodcastAction;
use crate::ffi::actions::publish_module::PublishAction;
use crate::ffi::actions::queue_module::QueueAction;
use crate::ffi::actions::settings_module::SettingsAction;
use crate::ffi::actions::siri_module::SiriAction;
use crate::ffi::actions::social_module::SocialAction;
use crate::ffi::actions::tasks_module::AgentTasksAction;
use crate::ffi::actions::voice_module::VoiceAction;
use crate::ffi::actions::wiki_module::WikiAction;
use crate::ffi::handle::OwnedPublishState;
use crate::ffi::projections::{
    AgentNoteSummary, AgentPickSummary, AgentTaskSummary, CommentSummary, KnowledgeSearchResult,
    NostrShowSummary, PodcastSummary, SocialSnapshot, TranscriptEntry, VoiceState, WikiArticle,
};
use crate::host_op_handler_queue::handle_queue_action;
use crate::host_op_publish::handle_publish_action;
use crate::identity_handler::IdentityHandler;
use crate::inbox_handler::{handle_inbox_action, handle_inbox_action_with_signal};
use crate::inbox_llm::TriageResult;
use crate::memory_handler;
use crate::picks_handler::{
    handle_refresh as picks_handle_refresh,
    handle_refresh_with_signal as picks_handle_refresh_with_signal,
};
use crate::player::PlayerActor;
use crate::queue::PlaybackQueue;
use crate::snapshot_signal::SnapshotUpdateSignal;
use crate::store::identity::IdentityStore;
use crate::store::{PodcastKeyStore, PodcastStore};
use crate::voice_handler;
use crate::wiki::{handle_wiki_action, handle_wiki_action_with_signal};

mod dispatch;
mod player_actions;
mod podcast_action_dispatch;
mod podcast_actions;
mod podcast_actions_downloads;
mod podcast_actions_feed;
mod podcast_actions_refresh;
mod settings_actions;
mod siri_actions;
mod social_actions;
mod task_actions;

/// Kernel-side handler owning every `Arc`d state slot the snapshot reader
/// (in `ffi::handle::PodcastHandle`) projects, plus the `*mut NmpApp` used
/// to dispatch capability requests back into the iOS executor.
///
/// Construction mirrors `PodcastHandle` field-for-field (see
/// `ffi::register::nmp_app_podcast_register`), with one exception:
/// `agent_chat` is the already-constructed `AgentChatHandler`.
pub struct PodcastHostOpHandler {
    pub(crate) app: *mut NmpApp,
    pub(crate) store: Arc<Mutex<PodcastStore>>,
    pub(crate) identity: Arc<Mutex<IdentityStore>>,
    pub(crate) player_actor: Arc<Mutex<PlayerActor>>,
    pub(crate) search_results: Arc<Mutex<Vec<PodcastSummary>>>,
    pub(crate) nostr_results: Arc<Mutex<Vec<NostrShowSummary>>>,
    pub(crate) queue: Arc<Mutex<PlaybackQueue>>,
    pub(crate) download_queue: Arc<Mutex<DownloadQueue>>,
    pub(crate) wiki_articles: Arc<Mutex<Vec<WikiArticle>>>,
    pub(crate) wiki_search_results: Arc<Mutex<Vec<WikiArticle>>>,
    pub(crate) picks: Arc<Mutex<Vec<AgentPickSummary>>>,
    /// Re-entrancy guard for background LLM picks scoring (M5.6); see
    /// `picks_handler::handle_refresh`. Handler-only (the snapshot never reads
    /// it, so it is not mirrored onto `PodcastHandle`); set in `new()`.
    pub(crate) picks_score_in_progress: Arc<std::sync::atomic::AtomicBool>,
    pub(crate) agent_tasks: Arc<Mutex<Vec<AgentTaskSummary>>>,
    pub(crate) knowledge_search_results: Arc<Mutex<Vec<KnowledgeSearchResult>>>,
    /// RAG chunk store (M5.3). Shared with `PodcastHandle.knowledge_store`.
    pub(crate) knowledge_store: Arc<Mutex<podcast_knowledge::KnowledgeStore>>,
    pub(crate) clips: Arc<Mutex<Vec<ClipRecord>>>,
    pub(crate) transcripts: Arc<Mutex<HashMap<String, Vec<TranscriptEntry>>>>,
    pub(crate) dismissed_episode_ids: Arc<Mutex<HashSet<String>>>,
    pub(crate) voice_state: Arc<Mutex<VoiceState>>,
    /// Categorizer cache shared with
    /// `ffi::handle::PodcastHandle::categories`. Mutated by
    /// `handle_categorize_*` and auto-triggered at the end of every
    /// successful feed refresh. Phase-1 keyword tags are written
    /// synchronously; the background LLM pass (M5.6) re-stamps entries.
    pub(crate) categories: Arc<Mutex<HashMap<String, Vec<String>>>>,
    /// Re-entrancy guard for the background LLM categorization pass. Set
    /// `true` when a pass is spawned, cleared when it finishes, so a feed
    /// refresh fired while the previous LLM pass is still running doesn't
    /// race a second one on the shared `categories` cache. Internal only —
    /// not projected to `PodcastUpdate`.
    pub(crate) categorization_in_progress: Arc<std::sync::atomic::AtomicBool>,
    pub(crate) rev: Arc<AtomicU64>,
    /// Per-podcast Nostr keypairs for NIP-F4 owned podcasts. Shared with
    /// `PodcastHandle.podcast_keys` so the snapshot reader sees the same
    /// data.
    pub(crate) podcast_keys: Arc<Mutex<PodcastKeyStore>>,
    /// Diagnostic publish state per podcast (last show event JSON +
    /// last-published timestamp). Shared with `PodcastHandle.publish_state`.
    pub(crate) publish_state: Arc<Mutex<HashMap<String, OwnedPublishState>>>,
    pub(crate) agent_chat: AgentChatHandler,
    /// NIP-22 (kind 1111) comment cache, keyed by episode_id string.
    /// Written by `handle_fetch_comments` / `handle_post_comment` on the
    /// actor thread; read by `build_snapshot_payload` on the main thread.
    /// In-memory only — comments re-fetch on next `FetchComments` dispatch.
    pub(crate) comments_cache: Arc<Mutex<HashMap<String, Vec<CommentSummary>>>>,
    /// Episode id whose comments the user is currently viewing. Shared with
    /// `PodcastHandle.viewed_comments_episode_id`. Set by
    /// `handle_fetch_comments` so the snapshot reader projects the viewed
    /// episode's comments rather than the now-playing episode's.
    pub(crate) viewed_comments_episode_id: Arc<Mutex<Option<String>>>,
    /// Shared Tokio runtime for async LLM / relay work. Seeded in
    /// `ffi::register` so all host-op handlers share one multi-thread scheduler.
    /// Used by wiki synthesis, agent chat, inbox triage, and social graph fetches.
    pub(crate) runtime: Arc<Runtime>,
    /// In-memory triage cache: `episode_id -> TriageResult`.
    ///
    /// Populated by `InboxAction::Triage` on the actor thread (running LLM
    /// triage for each unlistened episode) and read by `build_inbox` to
    /// overlay LLM scores over the recency-bucket fallback. Shared with
    /// `PodcastHandle.inbox_triage_cache` so the snapshot reader sees
    /// results without holding the handler lock.
    pub(crate) inbox_triage_cache: Arc<Mutex<HashMap<String, TriageResult>>>,
    /// Shared with `PodcastHandle.inbox_triage_in_progress`; set `true` when a
    /// background triage task starts, cleared when it finishes.
    pub(crate) inbox_triage_in_progress: Arc<std::sync::atomic::AtomicBool>,
    /// Active social-graph snapshot, populated by `FetchContacts`. Shared
    /// with `PodcastHandle.social` so the snapshot reader projects it on
    /// every tick after the first fetch.
    pub(crate) social: Arc<Mutex<Option<SocialSnapshot>>>,
    /// Feature #44 — inbound agent-to-agent kind:1 notes addressed to the
    /// active account, populated by `FetchAgentNotes`. Shared with
    /// `PodcastHandle.agent_notes` so the snapshot reader projects them on
    /// `PodcastUpdate.agent_notes` (reactive push seam — no polling).
    /// In-memory only; re-fetched on the next `FetchAgentNotes` dispatch.
    pub(crate) agent_notes: Arc<Mutex<Vec<AgentNoteSummary>>>,
    /// Coordinates optimistic-subscribe async feed fetches. Shared with
    /// `PodcastHandle` (whose HTTP-report FFI applies the results); this handler
    /// registers a pending fetch then fire-and-forget dispatches the async HTTP
    /// command on the actor thread.
    pub(crate) feed_fetch: Arc<FeedFetchCoordinator>,
    /// App-scoped feedback runtime. Shared with `PodcastHandle` so actions,
    /// observer pushes, and snapshots read the same cache.
    pub(crate) feedback: nmp_feedback::FeedbackRuntime,
    pub(crate) snapshot_signal: Option<SnapshotUpdateSignal>,
}

// SAFETY: the auto-derived `!Send`/`!Sync` comes solely from the
// `app: *mut NmpApp` field. The same caller contract documented on
// `PodcastHandle` applies here: Swift only dispatches host-ops on the
// actor thread, and `nmp_app_free` joins the actor before dropping the
// allocation, fencing any in-flight callbacks.
unsafe impl Send for PodcastHostOpHandler {}
unsafe impl Sync for PodcastHostOpHandler {}

impl PodcastHostOpHandler {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        app: *mut NmpApp,
        store: Arc<Mutex<PodcastStore>>,
        identity: Arc<Mutex<IdentityStore>>,
        player_actor: Arc<Mutex<PlayerActor>>,
        search_results: Arc<Mutex<Vec<PodcastSummary>>>,
        nostr_results: Arc<Mutex<Vec<NostrShowSummary>>>,
        queue: Arc<Mutex<PlaybackQueue>>,
        download_queue: Arc<Mutex<DownloadQueue>>,
        wiki_articles: Arc<Mutex<Vec<WikiArticle>>>,
        wiki_search_results: Arc<Mutex<Vec<WikiArticle>>>,
        picks: Arc<Mutex<Vec<AgentPickSummary>>>,
        agent_tasks: Arc<Mutex<Vec<AgentTaskSummary>>>,
        knowledge_search_results: Arc<Mutex<Vec<KnowledgeSearchResult>>>,
        knowledge_store: Arc<Mutex<podcast_knowledge::KnowledgeStore>>,
        clips: Arc<Mutex<Vec<ClipRecord>>>,
        transcripts: Arc<Mutex<HashMap<String, Vec<TranscriptEntry>>>>,
        dismissed_episode_ids: Arc<Mutex<HashSet<String>>>,
        voice_state: Arc<Mutex<VoiceState>>,
        categories: Arc<Mutex<HashMap<String, Vec<String>>>>,
        rev: Arc<AtomicU64>,
        podcast_keys: Arc<Mutex<PodcastKeyStore>>,
        publish_state: Arc<Mutex<HashMap<String, OwnedPublishState>>>,
        agent_chat: AgentChatHandler,
        comments_cache: Arc<Mutex<HashMap<String, Vec<CommentSummary>>>>,
        viewed_comments_episode_id: Arc<Mutex<Option<String>>>,
        runtime: Arc<Runtime>,
        inbox_triage_cache: Arc<Mutex<HashMap<String, TriageResult>>>,
        inbox_triage_in_progress: Arc<std::sync::atomic::AtomicBool>,
        social: Arc<Mutex<Option<SocialSnapshot>>>,
        agent_notes: Arc<Mutex<Vec<AgentNoteSummary>>>,
        feed_fetch: Arc<FeedFetchCoordinator>,
        feedback: nmp_feedback::FeedbackRuntime,
    ) -> Self {
        Self {
            app,
            store,
            identity,
            player_actor,
            search_results,
            nostr_results,
            queue,
            download_queue,
            wiki_articles,
            wiki_search_results,
            picks,
            picks_score_in_progress: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            categorization_in_progress: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            agent_tasks,
            knowledge_search_results,
            knowledge_store,
            clips,
            transcripts,
            dismissed_episode_ids,
            voice_state,
            categories,
            rev,
            podcast_keys,
            publish_state,
            agent_chat,
            comments_cache,
            viewed_comments_episode_id,
            runtime,
            inbox_triage_cache,
            inbox_triage_in_progress,
            social,
            agent_notes,
            feed_fetch,
            feedback,
            snapshot_signal: None,
        }
    }

    pub(crate) fn with_snapshot_signal(mut self, snapshot_signal: SnapshotUpdateSignal) -> Self {
        self.snapshot_signal = Some(snapshot_signal);
        self
    }

    /// Re-run the categorizer after a successful refresh so newly-
    /// arrived episodes pick up labels automatically.
    pub(super) fn auto_categorize(&self) {
        let _ = if let Some(signal) = self.snapshot_signal.clone() {
            categorization_run_with_signal(
                &self.store,
                &self.categories,
                &self.rev,
                &self.runtime,
                &self.categorization_in_progress,
                signal,
            )
        } else {
            categorization_run(
                &self.store,
                &self.categories,
                &self.rev,
                &self.runtime,
                &self.categorization_in_progress,
            )
        };
    }

    /// Re-run the AI picks pass after a successful refresh so newly-arrived
    /// episodes are folded into a fresh personalized ranking automatically —
    /// the same auto-trigger discipline as [`Self::auto_categorize`].
    ///
    /// This goes through [`picks_handle_refresh`] (not the bare
    /// `refresh_picks_into_slot` heuristic stamp) so the LLM scoring path
    /// actually runs in normal operation: the heuristic stamps the rail
    /// immediately and the background LLM pass upgrades it. The
    /// `picks_score_in_progress` guard coalesces the repeated calls that a
    /// `refresh_all` batch would otherwise produce into a single scoring pass.
    pub(super) fn auto_refresh_picks(&self) {
        let _ = if let Some(signal) = self.snapshot_signal.clone() {
            picks_handle_refresh_with_signal(
                &self.store,
                &self.picks,
                &self.rev,
                &self.runtime,
                &self.picks_score_in_progress,
                signal,
            )
        } else {
            picks_handle_refresh(
                &self.store,
                &self.picks,
                &self.rev,
                &self.runtime,
                &self.picks_score_in_progress,
            )
        };
    }
}

impl HostOpHandler for PodcastHostOpHandler {
    fn handle(&self, action_json: &str, correlation_id: &str) -> serde_json::Value {
        if let Ok(action) = serde_json::from_str::<IdentityAction>(action_json) {
            return IdentityHandler::new(self.identity.clone(), self.rev.clone()).handle(action);
        }
        if let Ok(action) = serde_json::from_str::<CategorizationAction>(action_json) {
            return match action {
                CategorizationAction::Run => {
                    if let Some(signal) = self.snapshot_signal.clone() {
                        categorization_run_with_signal(
                            &self.store,
                            &self.categories,
                            &self.rev,
                            &self.runtime,
                            &self.categorization_in_progress,
                            signal,
                        )
                    } else {
                        categorization_run(
                            &self.store,
                            &self.categories,
                            &self.rev,
                            &self.runtime,
                            &self.categorization_in_progress,
                        )
                    }
                }
                CategorizationAction::CategorizeEpisode { episode_id } => {
                    handle_categorize_episode(&self.store, &self.categories, &self.rev, episode_id)
                }
            };
        }
        if let Ok(action) = serde_json::from_str::<PodcastAction>(action_json) {
            return self.handle_podcast_action(action, correlation_id);
        }
        if let Ok(action) = serde_json::from_str::<PublishAction>(action_json) {
            return handle_publish_action(self, action);
        }
        if let Ok(action) = serde_json::from_str::<PlayerAction>(action_json) {
            return self.handle_player_action(action, correlation_id);
        }
        if let Ok(action) = serde_json::from_str::<InboxAction>(action_json) {
            return if let Some(signal) = self.snapshot_signal.clone() {
                handle_inbox_action_with_signal(
                    action,
                    &self.store,
                    &self.dismissed_episode_ids,
                    &self.rev,
                    &self.inbox_triage_cache,
                    &self.runtime,
                    &self.inbox_triage_in_progress,
                    signal,
                )
            } else {
                handle_inbox_action(
                    action,
                    &self.store,
                    &self.dismissed_episode_ids,
                    &self.rev,
                    &self.inbox_triage_cache,
                    &self.runtime,
                    &self.inbox_triage_in_progress,
                )
            };
        }
        if let Ok(action) = serde_json::from_str::<QueueAction>(action_json) {
            return handle_queue_action(&self.queue, &self.store, &self.rev, action);
        }
        if let Ok(action) = serde_json::from_str::<ChaptersAction>(action_json) {
            return match action {
                ChaptersAction::Compile { episode_id } => {
                    if let Some(signal) = self.snapshot_signal.clone() {
                        handle_compile_chapters_with_signal(
                            &self.store,
                            &self.rev,
                            &self.runtime,
                            episode_id,
                            signal,
                        )
                    } else {
                        handle_compile_chapters(&self.store, &self.rev, &self.runtime, episode_id)
                    }
                }
            };
        }
        if let Ok(action) = serde_json::from_str::<WikiAction>(action_json) {
            return if let Some(signal) = self.snapshot_signal.clone() {
                handle_wiki_action_with_signal(
                    &self.wiki_articles,
                    &self.wiki_search_results,
                    &self.store,
                    &self.knowledge_store,
                    &self.rev,
                    &self.runtime,
                    action,
                    signal,
                )
            } else {
                handle_wiki_action(
                    &self.wiki_articles,
                    &self.wiki_search_results,
                    &self.store,
                    &self.knowledge_store,
                    &self.rev,
                    &self.runtime,
                    action,
                )
            };
        }
        if let Ok(PicksAction::Refresh) = serde_json::from_str::<PicksAction>(action_json) {
            let p = &self.picks_score_in_progress;
            return if let Some(signal) = self.snapshot_signal.clone() {
                picks_handle_refresh_with_signal(
                    &self.store,
                    &self.picks,
                    &self.rev,
                    &self.runtime,
                    p,
                    signal,
                )
            } else {
                picks_handle_refresh(&self.store, &self.picks, &self.rev, &self.runtime, p)
            };
        }
        if let Ok(action) = serde_json::from_str::<AgentTasksAction>(action_json) {
            return self.handle_task_action(action);
        }
        if let Ok(a) = serde_json::from_str::<KnowledgeAction>(action_json) {
            return crate::knowledge::handle_knowledge_action(
                a,
                &self.store,
                &self.knowledge_search_results,
                &self.knowledge_store,
                &self.rev,
            );
        }
        if let Ok(action) = serde_json::from_str::<MemoryAction>(action_json) {
            return memory_handler::handle(action, &self.store, &self.rev);
        }
        if let Ok(action) = serde_json::from_str::<ClipAction>(action_json) {
            return ClipHandler::new(self.clips.clone(), self.store.clone(), self.rev.clone())
                .handle(action);
        }
        if let Ok(action) = serde_json::from_str::<VoiceAction>(action_json) {
            return voice_handler::handle(self, action, correlation_id);
        }
        if let Ok(action) = serde_json::from_str::<AgentChatAction>(action_json) {
            return self.agent_chat.handle(action);
        }
        if let Ok(action) = serde_json::from_str::<SettingsAction>(action_json) {
            return self.handle_settings_action(action);
        }
        if let Ok(action) = serde_json::from_str::<SiriAction>(action_json) {
            return self.handle_siri_action(action, correlation_id);
        }
        if let Ok(action) = serde_json::from_str::<SocialAction>(action_json) {
            return self.handle_social_action(action, correlation_id);
        }
        serde_json::json!({"ok": false, "error": format!("unknown action: {action_json}")})
    }
}
