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
//! * Task-action dispatch     -> `state::tasks::TasksState::handle` (Step 6)
//! * Inbox-action dispatch    -> `state::inbox::InboxState::handle` (Step 7)
//! * iTunes search helpers    -> `itunes.rs`
//! * `merge_episodes`         -> `host_op_handler_helpers.rs`
//! * Publish-action dispatch  -> `host_op_publish.rs`
//! * Voice-action dispatch    -> `voice_handler.rs`
//! * Namespace-envelope router -> `host_op_handler/router.rs`

use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use crate::state::PodcastAppState;

use tokio::runtime::Runtime;

use nmp_ffi::NmpApp;

use crate::download::DownloadQueue;
use crate::feed_fetch::FeedFetchCoordinator;
use crate::player::PlayerActor;
use crate::queue::PlaybackQueue;
use crate::snapshot_signal::SnapshotUpdateSignal;
use crate::store::identity::IdentityStore;
use crate::store::PodcastStore;

mod dispatch;
mod player_actions;
mod podcast_action_dispatch;
mod podcast_actions;
mod podcast_actions_downloads;
mod podcast_actions_feed;
mod podcast_actions_refresh;
mod router;
mod settings_actions;
mod siri_actions;
mod social_actions;
// task_actions removed in Step 6 — PodcastHostOpHandler::handle_task_action
// replaced by TasksState::handle in state/tasks.rs.

/// Kernel-side handler owning every `Arc`d state slot the snapshot reader
/// (in `ffi::handle::PodcastHandle`) projects, plus the `*mut NmpApp` used
/// to dispatch capability requests back into the iOS executor.
///
/// Construction mirrors `PodcastHandle` field-for-field (see
/// `ffi::register::nmp_app_podcast_register`), with one exception:
/// `agent_chat` is the already-constructed `AgentChatHandler`.
pub struct PodcastHostOpHandler {
    pub(crate) app: *mut NmpApp,
    /// Composed state root.  Inbox (Step 7), Knowledge (Step 1), Wiki (Step 2),
    /// Picks (Step 3), Categories (Step 4), Clips (Step 5a), Transcripts (Step 5b),
    /// Tasks (Step 6), Comments (Step 8), Discovery (Step 9), Social (Step 10),
    /// AgentChat (Step 11), Voice (Step 12), Publish (Step 13) all live here.
    pub(crate) state: Arc<PodcastAppState>,
    pub(crate) store: Arc<Mutex<PodcastStore>>,
    pub(crate) identity: Arc<Mutex<IdentityStore>>,
    pub(crate) player_actor: Arc<Mutex<PlayerActor>>,
    // search_results removed in Step 9 — now owned by `state.discovery` (DiscoveryState).
    // nostr_results removed in Step 9 — dead duplicate Arc; observer now shares
    // from `state.discovery.nostr_results`.
    pub(crate) queue: Arc<Mutex<PlaybackQueue>>,
    pub(crate) download_queue: Arc<Mutex<DownloadQueue>>,
    // wiki_articles and wiki_search_results removed in Step 2 —
    // they are now owned by `state.wiki` (WikiState).
    // picks + picks_score_in_progress removed in Step 3 —
    // they are now owned by `state.picks` (PicksState).
    // agent_tasks removed in Step 6 — now owned by `state.tasks` (TasksState).
    // knowledge_search_results and knowledge_store removed in Step 1 —
    // they are now owned by `state.knowledge` (KnowledgeState).
    // clips removed in Step 5a — now owned by `state.clips` (ClipsState).
    // transcripts removed in Step 5b — now owned by `state.transcripts` (TranscriptsState).
    // dismissed_episode_ids removed in Step 7 — now owned by `state.inbox` (InboxState).
    // voice_state removed in Step 12 — now owned by `state.voice` (VoiceSubstate).
    // categories + categorization_in_progress removed in Step 4 —
    // they are now owned by `state.categories` (CategoriesState).
    // comments_cache + viewed_comments_episode_id removed in Step 8 —
    // they are now owned by `state.comments` (CommentsState).
    // social removed in Step 10 — now owned by `state.social` (SocialState).
    // agent_notes removed in Step 10 — dead duplicate Arc; observer now shares
    // from `state.social.agent_notes`.
    pub(crate) rev: Arc<AtomicU64>,
    // podcast_keys and publish_state removed in Step 13 —
    // now owned by `state.publish` (PublishState).
    // agent_chat removed in Step 11 — now owned by `state.agent_chat` (AgentChatState).
    // inbox_triage_cache removed in Step 7 — now owned by `state.inbox` (InboxState).
    // inbox_triage_in_progress removed in Step 7 — now owned by `state.inbox` (InboxState).
    /// Shared Tokio runtime for async LLM / relay work. Seeded in
    /// `ffi::register` so all host-op handlers share one multi-thread scheduler.
    /// Used by wiki synthesis, agent chat, inbox triage, and social graph fetches.
    pub(crate) runtime: Arc<Runtime>,
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
        state: Arc<PodcastAppState>,
        store: Arc<Mutex<PodcastStore>>,
        identity: Arc<Mutex<IdentityStore>>,
        player_actor: Arc<Mutex<PlayerActor>>,
        queue: Arc<Mutex<PlaybackQueue>>,
        download_queue: Arc<Mutex<DownloadQueue>>,
        rev: Arc<AtomicU64>,
        runtime: Arc<Runtime>,
        feed_fetch: Arc<FeedFetchCoordinator>,
        feedback: nmp_feedback::FeedbackRuntime,
    ) -> Self {
        Self {
            app,
            state,
            store,
            identity,
            player_actor,
            queue,
            download_queue,
            rev,
            runtime,
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
    /// Step 4: delegates to CategoriesState (single canonical guard).
    pub(super) fn auto_categorize(&self) {
        let _ = self.state.categories.auto_run();
    }

    /// Re-run the AI picks pass after a successful refresh so newly-arrived
    /// episodes are folded into a fresh personalized ranking automatically.
    ///
    /// Delegates to `PicksState::auto_refresh` (Step 3 migration) which owns
    /// the single canonical `score_in_progress` guard and `infra.bump()`.
    pub(super) fn auto_refresh_picks(&self) {
        let _ = self.state.picks.auto_refresh();
    }

}
