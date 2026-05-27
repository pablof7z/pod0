//! Actor-thread handler for podcast/player host operations.
//!
//! Owns the kernel-side state every `podcast.*` action mutates and routes
//! incoming action JSON to the appropriate per-domain handler module.
//! Kept under the 500-LOC hard ceiling (AGENTS.md) by extracting:
//!
//! * Podcast/refresh dispatch -> `host_op_handler/podcast_actions.rs`
//! * Player-action dispatch   -> `host_op_handler/player_actions.rs`
//! * Queue-action dispatch    -> `host_op_handler_queue.rs`
//! * iTunes search helpers    -> `itunes.rs`
//! * `merge_episodes`         -> `host_op_handler_helpers.rs`
//! * Publish-action dispatch  -> `host_op_publish.rs`
//! * Voice-action dispatch    -> `voice_handler.rs`

use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use tokio::runtime::Runtime;

use nmp_core::substrate::{CapabilityRequest, HostOpHandler};
use nmp_ffi::NmpApp;

use crate::inbox_llm::TriageResult;
use crate::ad_skip_handler::handle_set_auto_skip_ads;
use crate::agent_handler::AgentChatHandler;
use crate::ai_chapters::handle_compile_chapters;
use crate::capability::{
    notification_command_json, AudioCommand, DownloadCommand, NotificationCommand,
    AUDIO_CAPABILITY_NAMESPACE, DOWNLOAD_CAPABILITY_NAMESPACE, NOTIFICATION_CAPABILITY_NAMESPACE,
};
use crate::categorization::{handle_categorize_episode, handle_run as categorization_run};
use crate::clip_handler::{ClipHandler, ClipRecord};
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
use crate::ffi::actions::tasks_module::AgentTasksAction;
use crate::ffi::actions::tts_module::TtsEpisodeAction;
use crate::ffi::actions::voice_module::VoiceAction;
use crate::ffi::actions::siri_module::SiriAction;
use crate::ffi::actions::wiki_module::WikiAction;
use crate::ffi::handle::OwnedPublishState;
use crate::ffi::projections::{
    AgentPickSummary, AgentTaskSummary, BriefingSnapshot, CommentSummary, KnowledgeSearchResult,
    NostrShowSummary, PodcastSummary, SocialSnapshot, TranscriptEntry, TtsEpisodeSummary,
    VoiceState, WikiArticle,
};
use crate::host_op_handler_queue::handle_queue_action;
use crate::host_op_publish::handle_publish_action;
use crate::identity_handler::IdentityHandler;
use crate::inbox_handler::handle_inbox_action;
use crate::store::identity::IdentityStore;
use crate::memory_handler;
use crate::picks_handler::handle_refresh as picks_handle_refresh;
use crate::player::PlayerActor;
use crate::download::DownloadQueue;
use crate::queue::PlaybackQueue;
use crate::store::{PodcastKeyStore, PodcastStore};
use crate::tasks_handler;
use crate::tts::TtsEpisodeHandler;
use crate::voice_handler;
use crate::wiki::handle_wiki_action;
use crate::capability::nostr_relay::{
    NostrRelayRequest, NostrRelayResult, NOSTR_RELAY_CAPABILITY_NAMESPACE,
};
use podcast_feeds::http::{HttpRequest, HttpResult, HTTP_CAPABILITY_NAMESPACE};

mod player_actions;
mod podcast_actions;
mod podcast_actions_feed;
mod siri_actions;

/// Kernel-side handler owning every `Arc`d state slot the snapshot reader
/// (in `ffi::handle::PodcastHandle`) projects, plus the `*mut NmpApp` used
/// to dispatch capability requests back into the iOS executor.
///
/// Construction mirrors `PodcastHandle` field-for-field (see
/// `ffi::register::nmp_app_podcast_register`), with two exceptions:
/// `tts` wraps `tts_episodes` in a `TtsEpisodeHandler` for namespace
/// hygiene, and `agent_chat` is the already-constructed
/// `AgentChatHandler`.
pub struct PodcastHostOpHandler {
    pub(crate) app: *mut NmpApp,
    pub(crate) store: Arc<Mutex<PodcastStore>>,
    pub(crate) identity: Arc<Mutex<IdentityStore>>,
    pub(crate) player_actor: Arc<Mutex<PlayerActor>>,
    pub(crate) search_results: Arc<Mutex<Vec<PodcastSummary>>>,
    pub(crate) nostr_results: Arc<Mutex<Vec<NostrShowSummary>>>,
    pub(crate) briefing: Arc<Mutex<Option<BriefingSnapshot>>>,
    pub(crate) queue: Arc<Mutex<PlaybackQueue>>,
    pub(crate) download_queue: Arc<Mutex<DownloadQueue>>,
    pub(crate) wiki_articles: Arc<Mutex<Vec<WikiArticle>>>,
    pub(crate) wiki_search_results: Arc<Mutex<Vec<WikiArticle>>>,
    pub(crate) picks: Arc<Mutex<Vec<AgentPickSummary>>>,
    pub(crate) agent_tasks: Arc<Mutex<Vec<AgentTaskSummary>>>,
    pub(crate) knowledge_search_results: Arc<Mutex<Vec<KnowledgeSearchResult>>>,
    pub(crate) tts: TtsEpisodeHandler,
    pub(crate) clips: Arc<Mutex<Vec<ClipRecord>>>,
    pub(crate) transcripts: Arc<Mutex<HashMap<String, Vec<TranscriptEntry>>>>,
    pub(crate) dismissed_episode_ids: Arc<Mutex<HashSet<String>>>,
    pub(crate) voice_state: Arc<Mutex<VoiceState>>,
    /// Heuristic categorizer cache shared with
    /// `ffi::handle::PodcastHandle::categories`. Mutated by
    /// `handle_categorize_*` and auto-triggered at the end of every
    /// successful feed refresh.
    pub(crate) categories: Arc<Mutex<HashMap<String, Vec<String>>>>,
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
    /// Active social-graph snapshot, populated by `FetchContacts`. Shared
    /// with `PodcastHandle.social` so the snapshot reader projects it on
    /// every tick after the first fetch.
    pub(crate) social: Arc<Mutex<Option<SocialSnapshot>>>,
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
        briefing: Arc<Mutex<Option<BriefingSnapshot>>>,
        queue: Arc<Mutex<PlaybackQueue>>,
        download_queue: Arc<Mutex<DownloadQueue>>,
        wiki_articles: Arc<Mutex<Vec<WikiArticle>>>,
        wiki_search_results: Arc<Mutex<Vec<WikiArticle>>>,
        picks: Arc<Mutex<Vec<AgentPickSummary>>>,
        agent_tasks: Arc<Mutex<Vec<AgentTaskSummary>>>,
        knowledge_search_results: Arc<Mutex<Vec<KnowledgeSearchResult>>>,
        tts_episodes: Arc<Mutex<Vec<TtsEpisodeSummary>>>,
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
        runtime: Arc<Runtime>,
        inbox_triage_cache: Arc<Mutex<HashMap<String, TriageResult>>>,
        social: Arc<Mutex<Option<SocialSnapshot>>>,
    ) -> Self {
        let tts = TtsEpisodeHandler::new(app, tts_episodes, rev.clone());
        Self {
            app,
            store,
            identity,
            player_actor,
            search_results,
            nostr_results,
            briefing,
            queue,
            download_queue,
            wiki_articles,
            wiki_search_results,
            picks,
            agent_tasks,
            knowledge_search_results,
            tts,
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
            runtime,
            inbox_triage_cache,
            social,
        }
    }

    /// Re-run the categorizer after a successful refresh so newly-
    /// arrived episodes pick up labels automatically.
    pub(super) fn auto_categorize(&self) {
        let _ = categorization_run(&self.store, &self.categories, &self.rev);
    }

    pub(crate) fn dispatch_http(
        &self,
        req: &HttpRequest,
        correlation_id: &str,
    ) -> Result<HttpResult, String> {
        let payload_json = serde_json::to_string(req).map_err(|e| e.to_string())?;
        let cap_req = CapabilityRequest {
            namespace: HTTP_CAPABILITY_NAMESPACE.to_owned(),
            correlation_id: correlation_id.to_owned(),
            payload_json,
        };
        let envelope = unsafe { &*self.app }.dispatch_capability(&cap_req);
        serde_json::from_str::<HttpResult>(&envelope.result_json)
            .map_err(|e| format!("decode http result: {e}"))
    }

    /// Dispatch a `nostr_relay` capability request and decode the result.
    ///
    /// Used by the `podcast.discover_nostr` handler (and publish handlers
    /// once wired). Mirrors `dispatch_http` — the capability executor
    /// (iOS shell or headless host) routes by namespace and returns a
    /// `CapabilityEnvelope` whose `result_json` is a `NostrRelayResult`.
    pub(crate) fn dispatch_nostr_relay(
        &self,
        req: &NostrRelayRequest,
        correlation_id: &str,
    ) -> Result<NostrRelayResult, String> {
        let payload_json = serde_json::to_string(req).map_err(|e| e.to_string())?;
        let cap_req = CapabilityRequest {
            namespace: NOSTR_RELAY_CAPABILITY_NAMESPACE.to_owned(),
            correlation_id: correlation_id.to_owned(),
            payload_json,
        };
        let envelope = unsafe { &*self.app }.dispatch_capability(&cap_req);
        serde_json::from_str::<NostrRelayResult>(&envelope.result_json)
            .map_err(|e| format!("decode nostr_relay result: {e}"))
    }

    pub(crate) fn dispatch_audio(
        &self,
        cmd: &AudioCommand,
        correlation_id: &str,
    ) -> Result<(), String> {
        let payload_json = serde_json::to_string(cmd).map_err(|e| e.to_string())?;
        let req = CapabilityRequest {
            namespace: AUDIO_CAPABILITY_NAMESPACE.to_owned(),
            correlation_id: correlation_id.to_owned(),
            payload_json,
        };
        let _ = unsafe { &*self.app }.dispatch_capability(&req);
        Ok(())
    }

    pub(super) fn dispatch_download(
        &self,
        cmd: &DownloadCommand,
        correlation_id: &str,
    ) -> Result<(), String> {
        let payload_json = serde_json::to_string(cmd).map_err(|e| e.to_string())?;
        let req = CapabilityRequest {
            namespace: DOWNLOAD_CAPABILITY_NAMESPACE.to_owned(),
            correlation_id: correlation_id.to_owned(),
            payload_json,
        };
        let _ = unsafe { &*self.app }.dispatch_capability(&req);
        Ok(())
    }

    /// Fire-and-forget notification dispatch. Mirrors the audio/download
    /// envelope shape so the iOS-side router can fan out by namespace
    /// without special-casing.
    pub(super) fn dispatch_notification(
        &self,
        cmd: &NotificationCommand,
        correlation_id: &str,
    ) -> Result<(), String> {
        let payload_json = notification_command_json(cmd);
        let req = CapabilityRequest {
            namespace: NOTIFICATION_CAPABILITY_NAMESPACE.to_owned(),
            correlation_id: correlation_id.to_owned(),
            payload_json,
        };
        let _ = unsafe { &*self.app }.dispatch_capability(&req);
        Ok(())
    }

    pub(crate) fn handle_settings_action(&self, action: SettingsAction) -> serde_json::Value {
        match action {
            SettingsAction::SetAutoSkipAds { enabled } => {
                handle_set_auto_skip_ads(&self.store, &self.player_actor, &self.rev, enabled)
            }
            SettingsAction::SetSkipIntervals { forward_secs, backward_secs } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_skip_intervals(forward_secs, backward_secs);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
        }
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
                    categorization_run(&self.store, &self.categories, &self.rev)
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
            return handle_inbox_action(
                action,
                &self.store,
                &self.dismissed_episode_ids,
                &self.rev,
                &self.inbox_triage_cache,
                &self.runtime,
            );
        }
        if let Ok(action) = serde_json::from_str::<QueueAction>(action_json) {
            return handle_queue_action(&self.queue, &self.store, &self.rev, action);
        }
        if let Ok(action) = serde_json::from_str::<ChaptersAction>(action_json) {
            return match action {
                ChaptersAction::Compile { episode_id } => {
                    handle_compile_chapters(&self.store, &self.rev, episode_id)
                }
            };
        }
        if let Ok(action) = serde_json::from_str::<WikiAction>(action_json) {
            return handle_wiki_action(
                &self.wiki_articles,
                &self.wiki_search_results,
                &self.store,
                &self.rev,
                &self.runtime,
                action,
            );
        }
        if let Ok(PicksAction::Refresh) = serde_json::from_str::<PicksAction>(action_json) {
            return picks_handle_refresh(&self.store, &self.picks, &self.rev);
        }
        if let Ok(action) = serde_json::from_str::<AgentTasksAction>(action_json) {
            return tasks_handler::handle_tasks_action(action, &self.agent_tasks, &self.rev);
        }
        if let Ok(a) = serde_json::from_str::<KnowledgeAction>(action_json) {
            return crate::knowledge::handle_knowledge_action(
                a,
                &self.store,
                &self.knowledge_search_results,
                &self.rev,
            );
        }
        if let Ok(action) = serde_json::from_str::<MemoryAction>(action_json) {
            return memory_handler::handle(action, &self.store, &self.rev);
        }
        if let Ok(action) = serde_json::from_str::<TtsEpisodeAction>(action_json) {
            return self.tts.handle(action, correlation_id);
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
        serde_json::json!({"ok": false, "error": format!("unknown action: {action_json}")})
    }
}
