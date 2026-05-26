//! The `pub extern "C"` registration entry point Swift links against to wire
//! Podcast projections and action namespaces into an [`NmpApp`].

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};


use nmp_ffi::NmpApp;

use super::actions::agent_module::AgentActionModule;
use super::actions::categorization_module::CategorizationModule;
use super::actions::chapters_module::ChaptersActionModule;
use super::actions::clip_module::ClipActionModule;
use super::actions::inbox_module::InboxActionModule;
use super::actions::knowledge_module::KnowledgeActionModule;
use super::actions::memory_module::MemoryActionModule;
use super::actions::picks_module::AgentPicksModule;
use super::actions::player_module::PlayerActionModule;
use super::actions::podcast_module::PodcastActionModule;
use super::actions::publish_module::NipF4PublishModule;
use super::actions::queue_module::QueueActionModule;
use super::actions::settings_module::SettingsActionModule;
use super::actions::siri_module::SiriActionModule;
use super::actions::tasks_module::AgentTasksModule;
use super::actions::tts_module::TtsEpisodeModule;
use super::actions::voice_module::VoiceActionModule;
use super::actions::wiki_module::WikiActionModule;
use super::handle::PodcastHandle;
use super::projections::VoiceState;
use crate::agent_handler::AgentChatHandler;
use crate::download::DownloadQueue;
use crate::host_op_handler::PodcastHostOpHandler;
use crate::player::PlayerActor;
use crate::queue::PlaybackQueue;
use crate::store::{PodcastKeyStore, PodcastStore};
use crate::tasks_handler;

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

    app_mut.register_action::<PodcastActionModule>();
    app_mut.register_action::<PlayerActionModule>();
    app_mut.register_action::<QueueActionModule>();
    app_mut.register_action::<ChaptersActionModule>();
    app_mut.register_action::<WikiActionModule>();
    app_mut.register_action::<AgentPicksModule>();
    app_mut.register_action::<AgentTasksModule>();
    app_mut.register_action::<KnowledgeActionModule>();
    app_mut.register_action::<MemoryActionModule>();
    app_mut.register_action::<TtsEpisodeModule>();
    app_mut.register_action::<ClipActionModule>();
    app_mut.register_action::<InboxActionModule>();
    app_mut.register_action::<NipF4PublishModule>();
    app_mut.register_action::<VoiceActionModule>();
    app_mut.register_action::<AgentActionModule>();
    app_mut.register_action::<CategorizationModule>();
    app_mut.register_action::<SettingsActionModule>();
    app_mut.register_action::<SiriActionModule>();

    // Shared state between the handle (snapshot reader) and the handler (writer).
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let player_actor = Arc::new(Mutex::new(PlayerActor::new()));
    let search_results = Arc::new(Mutex::new(Vec::new()));
    let nostr_results = Arc::new(Mutex::new(Vec::new()));
    let briefing = Arc::new(Mutex::new(None));
    let queue = Arc::new(Mutex::new(PlaybackQueue::new()));
    let download_queue = Arc::new(Mutex::new(DownloadQueue::new()));
    let wiki_articles = Arc::new(Mutex::new(Vec::new()));
    let wiki_search_results = Arc::new(Mutex::new(Vec::new()));
    let picks = Arc::new(Mutex::new(Vec::new()));
    // Seed the tasks slot with the two defaults so the iOS UI has rows
    // to render before the user has scheduled anything (see
    // `tasks_handler::default_seed`).
    let agent_tasks = Arc::new(Mutex::new(tasks_handler::default_seed()));
    let knowledge_search_results = Arc::new(Mutex::new(Vec::new()));
    let tts_episodes = Arc::new(Mutex::new(Vec::new()));
    let clips = Arc::new(Mutex::new(Vec::new()));
    let transcripts = Arc::new(Mutex::new(HashMap::new()));
    let dismissed_episode_ids = Arc::new(Mutex::new(HashSet::new()));
    let podcast_keys = Arc::new(Mutex::new(PodcastKeyStore::new()));
    let publish_state = Arc::new(Mutex::new(HashMap::new()));
    let voice_state = Arc::new(Mutex::new(VoiceState::default()));
    let conversation = Arc::new(Mutex::new(Vec::new()));
    let agent_busy = Arc::new(AtomicBool::new(false));
    let agent_touched = Arc::new(AtomicBool::new(false));
    let categories: Arc<Mutex<HashMap<String, Vec<String>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    // Start at 1 so the first snapshot poll always triggers an iOS update
    // (guard is `update.rev > last_seen_rev`; last_seen_rev starts at 0).
    // Subsequent increments happen in PodcastHostOpHandler on store writes.
    let rev = Arc::new(AtomicU64::new(1));

    let inbox_triage_cache = Arc::new(Mutex::new(HashMap::new()));

    let agent_chat = AgentChatHandler::new(
        conversation.clone(),
        agent_busy.clone(),
        agent_touched.clone(),
        rev.clone(),
    );

    // Shared Tokio runtime — multi-thread scheduler so async LLM/relay
    // work in future PRs can `.spawn` without a per-handler executor.
    let runtime = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .thread_name("podcast-tokio")
            .enable_all()
            .build()
            .expect("tokio runtime"),
    );

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
        download_queue.clone(),
        wiki_articles.clone(),
        wiki_search_results.clone(),
        picks.clone(),
        agent_tasks.clone(),
        knowledge_search_results.clone(),
        tts_episodes.clone(),
        clips.clone(),
        transcripts.clone(),
        dismissed_episode_ids.clone(),
        voice_state.clone(),
        categories.clone(),
        rev.clone(),
        podcast_keys.clone(),
        publish_state.clone(),
        agent_chat,
        runtime,
        inbox_triage_cache.clone(),
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
        download_queue,
        wiki_articles,
        wiki_search_results,
        picks,
        agent_tasks,
        knowledge_search_results,
        tts_episodes,
        clips,
        transcripts,
        dismissed_episode_ids,
        podcast_keys,
        publish_state,
        voice_state,
        conversation,
        agent_busy,
        agent_touched,
        categories,
        inbox_triage_cache,
    }))
}
