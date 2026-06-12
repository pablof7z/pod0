//! The `pub extern "C"` registration entry point Swift links against to wire
//! Podcast projections and action namespaces into an [`NmpApp`].

use std::sync::atomic::AtomicU64;
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
use super::guard::ffi_guard;
use super::handle::PodcastHandle;
use crate::host_op_handler::PodcastHostOpHandler;
use crate::snapshot_signal::SnapshotUpdateSignal;
use crate::store::identity::IdentityStore;
use crate::store::PodcastStore;
// player_actor, queue, download_queue removed in Step 14 —
// now seeded inside PodcastAppState::new (PlaybackState).

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
    ffi_guard("nmp_app_podcast_register", std::ptr::null_mut, || {
    // Wire the canonical NMP composition — NIP-02 / NIP-17 / NIP-57 / NIP-65
    // action modules, the kind:10050 ingest parser, the production routing
    // substrate, and the DM-inbox + zap-receipts runtime controllers.
    //
    // SAFETY: caller guarantees `app` is a valid pointer from `nmp_app_new`.
    // No other reference aliases it here — the `&*app` borrow further down is
    // taken only after this exclusive borrow is dropped.
    // AssertUnwindSafe is sound: all pointer null checks happen before this
    // closure is constructed; captured raw ptrs are never observed on panic path.
    let app_mut = unsafe { &mut *app };
    nmp_defaults::register_defaults(app_mut);

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
    // player_actor removed in Step 14 — now seeded inside PlaybackState.
    // search_results and nostr_results removed in Step 9 —
    // now owned by state.discovery (DiscoveryState).
    // queue removed in Step 14 — now seeded inside PlaybackState.
    // download_queue removed in Step 14 — now seeded inside PlaybackState.
    // wiki_articles and wiki_search_results removed in Step 2 —
    // they are now seeded inside PodcastAppState::new (WikiState).
    // picks and picks_score_in_progress removed in Step 3 —
    // they are now seeded inside PodcastAppState::new (PicksState).
    // clips removed in Step 5a — now seeded inside PodcastAppState::new (ClipsState).
    // transcripts removed in Step 5b — now seeded inside PodcastAppState::new (TranscriptsState).
    // agent_tasks removed in Step 6 — now seeded inside PodcastAppState::new (TasksState).
    // dismissed_episode_ids, inbox_triage_cache, inbox_triage_in_progress removed in Step 7 —
    // now seeded inside PodcastAppState::new (InboxState).
    // podcast_keys and publish_state removed in Step 13 —
    // now seeded inside PodcastAppState::new (PublishState).
    // voice_state and voice_conversation removed in Step 12 —
    // now seeded inside PodcastAppState::new via VoiceSubstate::new (with real app ptr).
    // conversation, agent_busy, agent_touched removed in Step 11 —
    // now seeded inside PodcastAppState::new (AgentChatState).
    // categories and categorization_in_progress removed in Step 4 —
    // they are now seeded inside PodcastAppState::new (CategoriesState).
    // comments_cache, viewed_comments_episode_id removed in Step 8 —
    // they are now owned by PodcastAppState::comments (CommentsState).
    // social, agent_notes removed in Steps 9-10 —
    // they are now owned by PodcastAppState::social (SocialState).
    // Start at 1 so the first snapshot poll always triggers an iOS update
    // (guard is `update.rev > last_seen_rev`; last_seen_rev starts at 0).
    // Subsequent increments happen in PodcastHostOpHandler on store writes.
    let rev = Arc::new(AtomicU64::new(1));

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

    // Steps 0-13 — composed state root.
    // `Infra` bundles rev + signal + runtime so substates can bump the
    // snapshot without receiving extra parameters.  `PodcastAppState::new`
    // seeds each substate's slots internally.  Both seams receive ONE Arc
    // clone; the old per-slot Arcs (knowledge/wiki/picks/agent_chat/voice slots
    // removed in Steps 1-3/11-12) are no longer needed in register.rs.
    //
    // Step 11: AgentChatState::new reads `infra.signal` to wire the snapshot
    // signal into its inner AgentChatHandler automatically.
    // Step 12: VoiceSubstate is constructed with null app by default inside
    // new_with_identity; replaced with the real app pointer below before
    // sealing into Arc.
    let app_state_infra = Infra {
        rev: rev.clone(),
        signal: Some(snapshot_signal.clone()),
        runtime: runtime.clone(),
    };
    // Steps 8-10: pass the shared identity Arc so CommentsState / SocialState
    // can access it without needing a separate clone in register.rs.
    let mut app_state_inner = PodcastAppState::new_with_identity(
        app_state_infra.clone(),
        store.clone(),
        identity.clone(),
    );
    // Step 12: Replace the default null-app VoiceSubstate with one that holds
    // the live `app` pointer so `VoiceConversationManager` can dispatch
    // `VoiceCommand::Speak` back to the iOS TTS executor.
    app_state_inner.voice = crate::state::voice::VoiceSubstate::new(
        app_state_infra,
        store.clone(),
        app,
    );
    let app_state = Arc::new(app_state_inner);

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

    // Steps 8-10: comments_cache, viewed_comments_episode_id, nostr_results,
    // Steps 8-10: search_results, social, agent_notes removed from constructor.
    // Step 11: agent_chat removed — now owned by state.agent_chat (AgentChatState).
    // Step 12: voice_state removed — now owned by state.voice (VoiceSubstate).
    // Step 13: podcast_keys + publish_state removed — now owned by state.publish (PublishState).
    // Step 7: dismissed_episode_ids, inbox_triage_cache, inbox_triage_in_progress removed —
    // now owned by state.inbox (InboxState).
    // Step 14: player_actor/queue/download_queue removed from constructor —
    // now owned by app_state.playback (PlaybackState).
    app_ref.set_host_op_handler(Arc::new(
        PodcastHostOpHandler::new(
            app,
            app_state.clone(),
            store.clone(),
            identity.clone(),
            rev.clone(),
            runtime.clone(),
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
    // Step 9: observer shares from state.discovery.nostr_results (removes the
    // dead-duplicate handler Arc from PodcastHostOpHandler.nostr_results).
    let _discovery_observer_id = app_ref.register_event_observer(std::sync::Arc::new(
        crate::discover_nostr::NostrDiscoveryObserver::new(
            app_state.discovery.nostr_results.share(),
            rev.clone(),
        )
        .with_snapshot_signal(snapshot_signal.clone()),
    ));

    // kind:1111 comments observer — receives events from push_interest_via_nmp
    // subscriptions opened by handle_fetch_comments. No iOS WebSocket.
    // Step 8: observer shares cache from state.comments.cache (removes the
    // dead-duplicate handler Arc from PodcastHostOpHandler.comments_cache).
    let _comments_observer_id = app_ref.register_event_observer(std::sync::Arc::new(
        crate::comments_handler::CommentsObserver::new(
            store.clone(),
            app_state.comments.cache.share(),
            rev.clone(),
        )
        .with_snapshot_signal(snapshot_signal.clone()),
    ));

    // kind:1 agent-notes observer — receives events from push_interest_via_nmp
    // subscriptions opened by handle_fetch_agent_notes. No iOS WebSocket.
    // Step 10: observer shares from state.social.agent_notes (removes the
    // dead-duplicate handler Arc from PodcastHostOpHandler.agent_notes).
    let _agent_notes_observer_id = app_ref.register_event_observer(std::sync::Arc::new(
        crate::agent_note_handler::AgentNotesObserver::new(
            identity.clone(),
            app_state.social.agent_notes.share(),
            rev.clone(),
        )
        .with_snapshot_signal(snapshot_signal.clone()),
    ));

    // In-app feedback observer. The reusable module owns event filtering,
    // bounded caching, and snapshot rev bumps. Unlike agent-notes, it does NOT
    // self-filter — the Feedback UI shows the user's own threads.
    let _feedback_observer_id =
        app_ref.register_event_observer(std::sync::Arc::new(feedback_runtime.observer()));

    // Step 12: voice_state + voice_conversation are now owned by state.voice (VoiceSubstate).
    // The runtime clone below is still needed for the snapshot path and off-actor work.
    let runtime_for_handle = runtime;

    // Steps 7-13: dismissed_episode_ids, inbox_triage_cache, inbox_triage_in_progress,
    // search_results, nostr_results, comments_cache, viewed_comments_episode_id, social,
    // agent_notes, voice_state, voice_conversation, podcast_keys, publish_state removed —
    // now owned by state.inbox / state.* respectively.
    let handle = Arc::new(PodcastHandle {
        app,
        state: app_state,
        // player_actor removed in Step 14 — now owned by state.playback.player.
        store,
        identity,
        rev,
        snapshot_signal: Some(snapshot_signal.clone()),
        snapshot_cache: Arc::new(Mutex::new(None)),
        clean_html_cache: Arc::new(Mutex::new(std::collections::HashMap::new())),
        // queue removed in Step 14 — now owned by state.playback.queue.
        // download_queue removed in Step 14 — now owned by state.playback.downloads.
        // clips, transcripts, agent_tasks removed in Steps 5a, 5b, 6 —
        // now owned by state.clips / state.transcripts / state.tasks.
        // dismissed_episode_ids, inbox_triage_cache, inbox_triage_in_progress removed in Step 7 —
        // now owned by state.inbox (InboxState).
        // podcast_keys and publish_state removed in Step 13 —
        // now owned by state.publish (PublishState).
        // conversation, agent_busy, agent_touched removed in Step 11 —
        // now owned by state.agent_chat (AgentChatState).
        // voice_state, voice_conversation removed in Step 12 —
        // now owned by state.voice (VoiceSubstate).
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
    }) // ffi_guard
}
