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

use std::sync::Arc;

use crate::state::PodcastAppState;

use nmp_ffi::NmpApp;
// AtomicU64, Mutex, Runtime, SnapshotUpdateSignal removed in Step N+1 —
// rev + signal + runtime are now accessed via state.infra.

mod dispatch;
mod player_actions;
mod podcast_action_dispatch;
mod podcast_actions;
mod podcast_actions_downloads;
#[cfg(test)]
mod podcast_actions_downloads_tests;
mod podcast_actions_feed;
mod podcast_actions_refresh;
#[cfg(test)]
mod podcast_actions_refresh_tests;
mod router;
mod settings_actions;
mod siri_actions;
mod social_actions;
// Task-action dispatch lives in state/tasks.rs (TasksState::handle).

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
    /// AgentChat (Step 11), Voice (Step 12), Publish (Step 13), Playback (Step 14),
    /// Library/identity (Step 15) all live here.
    pub(crate) state: Arc<PodcastAppState>,
    // store removed in Step 15 — now owned by `state.library.store`.
    // identity removed in Step 15 — now owned by `state.library.identity`.
    // player_actor removed in Step 14 — now owned by `state.playback.player`.
    // search_results removed in Step 9 — now owned by `state.discovery` (DiscoveryState).
    // nostr_results removed in Step 9 — dead duplicate Arc; observer now shares
    // from `state.discovery.nostr_results`.
    // queue removed in Step 14 — now owned by `state.playback.queue`.
    // download_queue removed in Step 14 — now owned by `state.playback.downloads`.
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
    // rev removed in Step N+1 — now accessed via `state.infra.rev`.
    // runtime removed in Step N+1 — now accessed via `state.infra.runtime`.
    // snapshot_signal removed in Step N+1 — now in `state.infra.signal`.
    // podcast_keys and publish_state removed in Step 13 —
    // now owned by `state.publish` (PublishState).
    // agent_chat removed in Step 11 — now owned by `state.agent_chat` (AgentChatState).
    // inbox_triage_cache removed in Step 7 — now owned by `state.inbox` (InboxState).
    // inbox_triage_in_progress removed in Step 7 — now owned by `state.inbox` (InboxState).
    // feed_fetch removed in Step 16 — now accessed via `state.feed_fetch`.
    // feedback removed in Step 16 — now accessed via `state.feedback`.
}

// SAFETY: the auto-derived `!Send`/`!Sync` comes solely from the
// `app: *mut NmpApp` field. The same caller contract documented on
// `PodcastHandle` applies here: Swift only dispatches host-ops on the
// actor thread, and `nmp_app_free` joins the actor before dropping the
// allocation, fencing any in-flight callbacks.
unsafe impl Send for PodcastHostOpHandler {}
unsafe impl Sync for PodcastHostOpHandler {}

impl PodcastHostOpHandler {
    /// Step N+1: The minimal 2-arg constructor.  All infrastructure
    /// (rev + signal + runtime) comes from `state.infra`.
    pub fn new(app: *mut NmpApp, state: Arc<PodcastAppState>) -> Self {
        Self { app, state }
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

    /// Bump the global rev AND a specific push domain's rev for an on-actor-thread
    /// mutation.
    ///
    /// Handler arms run synchronously on the actor thread during command
    /// dispatch — the actor emits a frame after the command returns, so these
    /// sites do NOT post the snapshot signal (matching the historical raw
    /// `infra.rev.fetch_add(1)` they replace). They DO advance the named domain
    /// rev so the per-domain typed sidecar fires its delta on that emit.
    ///
    /// This is the ONE place the handler's domain-bump mechanism lives; the
    /// `Domain` argument at each call site is the per-mutation mapping. A
    /// `podcast.settings.*` arm passes `Domain::Settings`, a player arm
    /// `Domain::Playback`, a feed/library arm `Domain::Library`, etc.
    pub(crate) fn bump_domain(&self, domain: crate::state::Domain) {
        use std::sync::atomic::Ordering;
        self.state
            .infra
            .domain_revs
            .counter(domain)
            .fetch_add(1, Ordering::Relaxed);
        self.state.infra.rev.fetch_add(1, Ordering::Relaxed);
    }
}
