//! The `pub extern "C"` registration entry point Swift links against to wire
//! Podcast projections and action namespaces into an [`NmpApp`].

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};

use crate::state::{Infra, PodcastAppState};

use nmp_ffi::NmpApp;

use super::snapshot::build_snapshot_payload;

use super::actions::agent_module::AgentActionModule;
use super::actions::categorization_module::CategorizationModule;
use super::actions::chapters_module::ChaptersActionModule;
use super::actions::clip_module::ClipActionModule;
use super::actions::identity_module::IdentityActionModule;
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
use super::actions::social_module::SocialActionModule;
use super::actions::tasks_module::AgentTasksModule;
use super::actions::voice_module::VoiceActionModule;
use super::actions::wiki_module::WikiActionModule;
use super::handle::PodcastHandle;
use super::projections::VoiceState;
use crate::agent_handler::AgentChatHandler;
use crate::download::DownloadQueue;
use crate::host_op_handler::PodcastHostOpHandler;
use crate::player::PlayerActor;
use crate::queue::PlaybackQueue;
use crate::snapshot_signal::SnapshotUpdateSignal;
use crate::store::identity::IdentityStore;
use crate::store::{PodcastKeyStore, PodcastStore};

/// Register Podcast projections and action namespaces against `app`. Returns a
/// non-null `*mut PodcastHandle` on success; `null` on any failure (null
/// pointer arguments, slot lock poisoning).
///
/// `app` MUST outlive the returned handle. Call
/// [`nmp_app_podcast_unregister`] before `nmp_app_free`.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_register(app: *mut NmpApp) -> *mut PodcastHandle {
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

    app_mut.register_action::<IdentityActionModule>();
    app_mut.register_action::<PodcastActionModule>();
    app_mut.register_action::<PlayerActionModule>();
    app_mut.register_action::<QueueActionModule>();
    app_mut.register_action::<ChaptersActionModule>();
    app_mut.register_action::<WikiActionModule>();
    app_mut.register_action::<AgentPicksModule>();
    app_mut.register_action::<AgentTasksModule>();
    app_mut.register_action::<KnowledgeActionModule>();
    app_mut.register_action::<MemoryActionModule>();
    app_mut.register_action::<ClipActionModule>();
    app_mut.register_action::<InboxActionModule>();
    app_mut.register_action::<NipF4PublishModule>();
    app_mut.register_action::<VoiceActionModule>();
    app_mut.register_action::<AgentActionModule>();
    app_mut.register_action::<CategorizationModule>();
    app_mut.register_action::<SettingsActionModule>();
    app_mut.register_action::<SiriActionModule>();
    app_mut.register_action::<SocialActionModule>();

    // Shared state between the handle (snapshot reader) and the handler (writer).
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let identity = Arc::new(Mutex::new(IdentityStore::new()));
    let player_actor = Arc::new(Mutex::new(PlayerActor::new()));
    let search_results = Arc::new(Mutex::new(Vec::new()));
    let nostr_results = Arc::new(Mutex::new(Vec::new()));
    let queue = Arc::new(Mutex::new(PlaybackQueue::new()));
    let download_queue = Arc::new(Mutex::new(DownloadQueue::new()));
    // wiki_articles and wiki_search_results removed in Step 2 —
    // they are now seeded inside PodcastAppState::new (WikiState).
    // picks and picks_score_in_progress removed in Step 3 —
    // they are now seeded inside PodcastAppState::new (PicksState).
    // clips removed in Step 5a — now seeded inside PodcastAppState::new (ClipsState).
    // transcripts removed in Step 5b — now seeded inside PodcastAppState::new (TranscriptsState).
    // agent_tasks removed in Step 6 — now seeded inside PodcastAppState::new (TasksState).
    let dismissed_episode_ids = Arc::new(Mutex::new(HashSet::new()));
    let podcast_keys = Arc::new(Mutex::new(PodcastKeyStore::new()));
    let publish_state = Arc::new(Mutex::new(HashMap::new()));
    let voice_state = Arc::new(Mutex::new(VoiceState::default()));
    let conversation = Arc::new(Mutex::new(Vec::new()));
    let agent_busy = Arc::new(AtomicBool::new(false));
    let agent_touched = Arc::new(AtomicBool::new(false));
    // categories and categorization_in_progress removed in Step 4 —
    // they are now seeded inside PodcastAppState::new (CategoriesState).
    let comments_cache: Arc<Mutex<HashMap<String, Vec<crate::ffi::projections::CommentSummary>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let viewed_comments_episode_id: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let social = Arc::new(Mutex::new(None));
    let agent_notes: Arc<Mutex<Vec<crate::ffi::projections::AgentNoteSummary>>> =
        Arc::new(Mutex::new(Vec::new()));
    // Start at 1 so the first snapshot poll always triggers an iOS update
    // (guard is `update.rev > last_seen_rev`; last_seen_rev starts at 0).
    // Subsequent increments happen in PodcastHostOpHandler on store writes.
    let rev = Arc::new(AtomicU64::new(1));

    let inbox_triage_cache = Arc::new(Mutex::new(HashMap::new()));
    let inbox_triage_in_progress = Arc::new(AtomicBool::new(false));

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
    let snapshot_signal = SnapshotUpdateSignal::new(rev.clone(), app_ref.actor_sender());
    let feedback_events_cache: nmp_feedback::FeedbackEventCache = Arc::new(Mutex::new(Vec::new()));
    let feedback_config =
        nmp_feedback::FeedbackConfig::new(crate::PODCAST_FEEDBACK_PROJECT_COORDINATE)
            .with_interest_namespace(crate::PODCAST_FEEDBACK_INTEREST_NAMESPACE);
    let feedback_runtime =
        nmp_feedback::FeedbackRuntime::new(feedback_config, feedback_events_cache, rev.clone())
            .with_snapshot_bump(Arc::new({
                let snapshot_signal = snapshot_signal.clone();
                move || snapshot_signal.bump()
            }));

    let agent_chat = AgentChatHandler::new(
        conversation.clone(),
        agent_busy.clone(),
        agent_touched.clone(),
        rev.clone(),
        runtime.clone(),
        store.clone(),
    )
    .with_snapshot_signal(snapshot_signal.clone());

    // Step 0-3 — composed state root.
    // `Infra` bundles rev + signal + runtime so substates can bump the
    // snapshot without receiving extra parameters.  `PodcastAppState::new`
    // seeds each substate's slots internally.  Both seams receive ONE Arc
    // clone; the old per-slot Arcs (knowledge/wiki/picks slots removed in
    // Steps 1-3) are no longer needed in register.rs for the migrated features.
    let app_state_infra = Infra {
        rev: rev.clone(),
        signal: Some(snapshot_signal.clone()),
        runtime: runtime.clone(),
    };
    let app_state = Arc::new(PodcastAppState::new(app_state_infra, store.clone()));

    // Optimistic-subscribe async feed-fetch coordinator. Shared (one `Arc`)
    // between the host-op handler (registers a pending fetch + dispatches the
    // async HTTP command on the actor thread) and the handle (whose HTTP-report
    // FFI applies the parsed result from the platform transport thread). Holds
    // the same shared `store` / `rev` / `runtime` Arcs the rest of the kernel
    // uses, plus the snapshot signal so it can re-project from off the actor thread.
    // Step 3: picks Arc shared from `app_state.picks` (single guard consolidation).
    // Step 4: categories Arc shared from `app_state.categories` (single guard
    // consolidation — eliminates the duplicate categorization_in_progress race).
    let feed_fetch = Arc::new(crate::feed_fetch::FeedFetchCoordinator::new(
        store.clone(),
        rev.clone(),
        Some(snapshot_signal.clone()),
        Arc::clone(&app_state.categories),
        Arc::clone(&app_state.picks),
    ));

    // Seed the podcast app's default relay set (NMP v0.2.1, PR #900).
    //
    // As of v0.2.1, `nmp-core` no longer carries a hardcoded onboarding relay
    // default — the app owns its relay list. The Rust composition root
    // (`NmpAppBuilder::start`) seeds `DEFAULT_APP_RELAYS` for builder-based
    // apps, but the podcast app is constructed by the iOS shell over the raw
    // C-ABI (`nmp_app_new` → `nmp_app_podcast_register` → `nmp_app_start`), so
    // it never runs through the builder. Without an explicit seed here a fresh
    // install would start with ZERO configured relays and Nostr discovery /
    // publish would silently no-op. `set_initial_relays_for_start` is the
    // non-builder seam: it stages `(url, role)` rows into
    // `ActorCommand::Start { initial_relays }`, read once by the actor before
    // the first tick. It takes `&self`, so it is sound on `app_ref`, and it
    // MUST run before the shell calls `nmp_app_start` (it does, after this
    // `register` returns). These two relays mirror the template's
    // `DEFAULT_APP_RELAYS`; the podcast app declares them explicitly.
    //
    // SEED-IF-EMPTY (step 4) — investigated, intentionally still unconditional.
    //
    // The `configured_relays` projection now exists and `podcast.settings`
    // exposes add/remove/set_role ops, so the obvious next step is to make this
    // seed run only on a fresh install. But a seed-if-empty guard is not
    // reachable from the raw C-ABI start path the podcast app uses, for two
    // compounding reasons:
    //
    //   1. At `register` time the slot is ALWAYS empty. The actor only
    //      populates `configured_relays` from `initial_relays` when it handles
    //      `ActorCommand::Start`, which runs AFTER `register` returns. So
    //      `unsafe { &*app }.configured_relays_handle().lock().is_empty()` here
    //      is unconditionally true — a guard reading it would be dead code.
    //
    //   2. There is no relay persistence on the C-ABI path. The v0.2.1
    //      relay-config sidecar (`relay_config::load`/`save`) is invoked ONLY
    //      inside `NmpAppBuilder::start`; the podcast app starts via the raw
    //      C-ABI (`nmp_app_new` → `nmp_app_podcast_register` → `nmp_app_start`)
    //      and `configured_relays` is in-memory kernel state that neither
    //      `kernel.start()` nor `restore_active_session` reloads from the LMDB
    //      store. So user relay edits do NOT survive an app restart, and there
    //      is no persisted state for a seed-if-empty to defer to.
    //
    // Net: this seed staging the two declared defaults is correct and harmless
    // — the slot is empty on every fresh process, so the seed never clobbers a
    // surviving user edit (there are none to clobber). Making relay edits
    // durable (and the seed genuinely first-install-only) requires wiring
    // relay-config sidecar persistence into the C-ABI start path; tracked in
    // BACKLOG (`relay-config-c-abi-persistence`).
    app_ref.set_initial_relays_for_start(vec![
        (
            "wss://relay.primal.net".to_string(),
            "both,indexer".to_string(),
        ),
        ("wss://purplepag.es".to_string(), "indexer".to_string()),
        // In-app feedback source relay. Seeded read-only so NMP opens the
        // connection used by the relay-pinned feedback subscription; publish
        // targets the same relay explicitly through `nmp-feedback`.
        feedback_runtime.config().relay_seed(),
    ]);

    app_ref.set_host_op_handler(Arc::new(
        PodcastHostOpHandler::new(
            app,
            app_state.clone(),
            store.clone(),
            identity.clone(),
            player_actor.clone(),
            search_results.clone(),
            nostr_results.clone(),
            queue.clone(),
            download_queue.clone(),
            dismissed_episode_ids.clone(),
            voice_state.clone(),
            rev.clone(),
            podcast_keys.clone(),
            publish_state.clone(),
            agent_chat,
            comments_cache.clone(),
            viewed_comments_episode_id.clone(),
            runtime.clone(),
            inbox_triage_cache.clone(),
            Arc::clone(&inbox_triage_in_progress),
            social.clone(),
            agent_notes.clone(),
            feed_fetch.clone(),
            feedback_runtime.clone(),
        )
        .with_snapshot_signal(snapshot_signal.clone()),
    ));

    // NIP-F4 discovery observer (canonical EnsureInterest + KernelEventObserver
    // pattern). The `podcast.discover_nostr` action emits
    // `ActorCommand::EnsureInterest` for `kind:10154`; NMP core opens the
    // subscription through its own relay pool (no iOS WebSocket — D7) and every
    // inbound show event fires this observer, which writes the projected show
    // onto the same `nostr_results` slot the snapshot reads. Registered before
    // the slot Arcs are moved into the handle. The returned id is dropped: the
    // observer lives for the app's lifetime (mirrors the snapshot projection),
    // and `nmp_app_free` joins the actor before dropping the slot.
    let _discovery_observer_id = app_ref.register_event_observer(std::sync::Arc::new(
        crate::discover_nostr::NostrDiscoveryObserver::new(nostr_results.clone(), rev.clone())
            .with_snapshot_signal(snapshot_signal.clone()),
    ));

    // kind:1111 comments observer — receives events from push_interest_via_nmp
    // subscriptions opened by handle_fetch_comments. No iOS WebSocket.
    let _comments_observer_id = app_ref.register_event_observer(std::sync::Arc::new(
        crate::comments_handler::CommentsObserver::new(
            store.clone(),
            comments_cache.clone(),
            rev.clone(),
        )
        .with_snapshot_signal(snapshot_signal.clone()),
    ));

    // kind:1 agent-notes observer — receives events from push_interest_via_nmp
    // subscriptions opened by handle_fetch_agent_notes. No iOS WebSocket.
    let _agent_notes_observer_id = app_ref.register_event_observer(std::sync::Arc::new(
        crate::agent_note_handler::AgentNotesObserver::new(
            identity.clone(),
            agent_notes.clone(),
            rev.clone(),
        )
        .with_snapshot_signal(snapshot_signal.clone()),
    ));

    // In-app feedback observer. The reusable module owns event filtering,
    // bounded caching, and snapshot rev bumps. Unlike agent-notes, it does NOT
    // self-filter — the Feedback UI shows the user's own threads.
    let _feedback_observer_id =
        app_ref.register_event_observer(std::sync::Arc::new(feedback_runtime.observer()));

    // Keep a clone for the handle before the runtime Arc is moved into the
    // voice manager below. The snapshot path's proactive triage trigger
    // (`maybe_enqueue_triage`) spawns onto this same shared runtime.
    let runtime_for_handle = runtime.clone();

    // Voice-mode conversation manager (M5.6-voice): owns the STT→LLM→TTS
    // turn history and dispatches LLM replies back to the iOS voice
    // executor. Holds clones of the shared store / voice-state / rev plus
    // the same Tokio runtime so background turns reuse the shared
    // scheduler. All clones of this `runtime` Arc (handler, voice manager,
    // handle) point at one runtime, so spawned work is fenced by the
    // actor-thread join before `nmp_app_free`.
    let voice_conversation = crate::voice_conversation::VoiceConversationManager::new(
        app,
        Arc::new(Mutex::new(Vec::new())),
        store.clone(),
        voice_state.clone(),
        runtime,
        rev.clone(),
        Some(snapshot_signal.clone()),
    );

    let handle = Arc::new(PodcastHandle {
        app,
        state: app_state,
        player_actor,
        store,
        identity,
        rev,
        snapshot_signal: Some(snapshot_signal.clone()),
        search_results,
        nostr_results,
        snapshot_cache: Arc::new(Mutex::new(None)),
        clean_html_cache: Arc::new(Mutex::new(std::collections::HashMap::new())),
        queue,
        download_queue,
        // clips, transcripts, agent_tasks removed in Steps 5a, 5b, 6 —
        // now owned by state.clips / state.transcripts / state.tasks.
        dismissed_episode_ids,
        podcast_keys,
        publish_state,
        voice_state,
        voice_conversation,
        conversation,
        agent_busy,
        agent_touched,
        inbox_triage_cache,
        inbox_triage_in_progress,
        comments_cache,
        viewed_comments_episode_id,
        social,
        agent_notes,
        feedback: feedback_runtime,
        runtime: runtime_for_handle,
        feed_fetch,
    });

    // Reactive push projection — the canonical snapshot-output seam
    // (`NmpApp::register_snapshot_projection_gated`). Podcast state now rides the
    // generic push frame under `projections["podcast.snapshot"]`, delivered to
    // the shell on every tick through the same update callback it already
    // listens on — replacing the bespoke `nmp_app_podcast_snapshot` pull symbol
    // and the shell's 500ms poll (a D8 violation / reborn deprecated
    // `chirp_snapshot` pattern).
    //
    // The closure runs on the actor thread inside `make_update` (D8 — must be
    // cheap, non-blocking). It reuses `build_snapshot_payload`, the SAME
    // serialization the pull path uses: that function owns the rev-gated
    // snapshot-string cache (so an unchanged `rev` is a cheap clone, not a
    // rebuild) AND the proven fallback-to-stub on a serialization error. Reusing
    // it makes the pushed projection byte-identical to the JSON the shell's pull
    // path already decodes successfully — avoiding a divergent `to_value(...)`
    // path that yields `null` (and a dropped frame) when the typed value can't
    // serialize (e.g. a non-finite float in real feed data).
    //
    // Change-gating (nmp-v0.2.10 / upstream PR #1068): `register_snapshot_projection_gated`
    // passes `handle.rev` (Arc<AtomicU64>) as the `ChangeGate`. The registry skips
    // re-invoking the closure — and therefore skips the full library
    // serialization — when `rev` is unchanged since the last emit. This is
    // the proper upstream fix superseding the interim local `value_cache`
    // approach: the gate check now lives in the registry, so the closure
    // body stays the plain always-correct fallback-to-stub form.
    {
        let proj = Arc::clone(&handle);
        let gate = Arc::clone(&handle.rev) as std::sync::Arc<dyn nmp_core::ChangeGate>;
        app_ref.register_snapshot_projection_gated("podcast.snapshot", gate, move || {
            serde_json::from_str(&build_snapshot_payload(&proj)).unwrap_or(serde_json::Value::Null)
        });
    }

    // Ownership: one strong ref is returned to the shell as the opaque handle
    // pointer; the projection closure above holds a second strong ref for the
    // app's lifetime. `nmp_app_podcast_unregister` reclaims the shell's ref via
    // `Arc::from_raw`. `nmp_app_free` joins the actor thread before dropping, so
    // no projector call is in flight after teardown. The handle is only ever
    // borrowed shared across the FFI (no `&mut`), so `Arc` aliasing is sound.
    Arc::into_raw(handle) as *mut PodcastHandle
}
