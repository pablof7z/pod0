//! The `pub extern "C"` registration entry point Swift links against to wire
//! Podcast projections and action namespaces into an [`NmpApp`].

use std::sync::{Arc, Mutex};

use crate::state::{Infra, PodcastAppState};

use nmp_ffi::NmpApp;
use nmp_nip02::ActiveFollowSet;

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

    // Wire the BUD-02 Blossom upload action (`nmp.blossom.upload`).
    // D13/D0: Rust owns the full Build → Sign → Transport pipeline.
    // Swift dispatches with a correlation-id and reads the BlobDescriptor
    // from action_results[correlation_id].result on the next push frame.
    nmp_blossom::register_actions(app_mut);

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
    // Install the host-op handler (requires &self, so take the ref AFTER the
    // &mut borrow above is released by the block end).
    let app_ref = unsafe { &*app };
    // Step N+1: rev + runtime are constructed here and bundled into Infra;
    // signal is also wired here and stored in infra.signal.  Neither is a
    // separate local variable anymore — the Infra is the canonical owner.
    //
    // Start at 1 so the first snapshot poll always triggers an iOS update
    // (guard is `update.rev > last_seen_rev`; last_seen_rev starts at 0).
    let rev = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(1));
    let snapshot_signal = SnapshotUpdateSignal::new(rev.clone(), app_ref.actor_sender());

    // Step 16: feedback runtime constructed here (needs the snapshot-bump hook
    // that captures the live signal) and injected into PodcastAppState.  The
    // observer and relay-seed calls below use app_state.feedback.
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

    // Steps 0-N+1 — composed state root.
    // `Infra` bundles rev + signal + runtime.  `PodcastAppState::new`
    // seeds each substate's slots internally.  Both seams receive ONE Arc
    // clone; no separate Arcs needed in register.rs.
    //
    // Step N+1: runtime is the LAST local Infra component; constructed here
    // and moved into Infra rather than stored as a separate `let runtime`.
    // Step 11: AgentChatState::new reads `infra.signal` to wire the snapshot
    // signal into its inner AgentChatHandler automatically.
    // Step 12: VoiceSubstate is constructed with null app by default inside
    // new_with_identity; replaced with the real app pointer below before
    // sealing into Arc.
    // Step 16: FeedFetchCoordinator is now constructed inside new_with_identity
    // (accesses picks + categories built there); FeedbackRuntime is injected.
    let app_state_infra = Infra {
        rev: rev.clone(),
        signal: Some(snapshot_signal.clone()),
        runtime: std::sync::Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .thread_name("podcast-tokio")
                .enable_all()
                .build()
                .expect("tokio runtime"),
        ),
        domain_revs: std::sync::Arc::new(crate::state::DomainRevs::new()),
        // Root infra defaults to Misc; each substate receives a domain-scoped
        // clone via `infra.with_domain(...)` inside PodcastAppState::new_with_identity.
        domain: crate::state::Domain::Misc,
    };
    // Steps 8-10: pass the shared identity Arc so CommentsState / SocialState
    // can access it without needing a separate clone in register.rs.
    let mut app_state_inner = PodcastAppState::new_with_identity(
        app_state_infra.clone(),
        store.clone(),
        identity.clone(),
        feedback_runtime,
    );
    // Step 12: Replace the default null-app VoiceSubstate with one that holds
    // the live `app` pointer so `VoiceConversationManager` can dispatch
    // `VoiceCommand::Speak` back to the iOS TTS executor.
    app_state_inner.voice = crate::state::voice::VoiceSubstate::new(
        app_state_infra,
        store.clone(),
        app,
    );

    // ── Reactive social-graph trust set ──────────────────────────────────────
    //
    // ActiveFollowSet (nmp-nip02): observes kind:3 events and maintains a live
    // BTreeSet<hex-pubkey> for the active account's follow list.  Its predicate
    // (`ActiveFollowSet::predicate()`) is a closure that captures the same
    // Arc<RwLock<…>> so consumers always see the latest kind:3 — no re-wiring.
    //
    // Constructed BEFORE sealing `app_state` so the SAME Arc can be injected
    // into SocialState (the projection recomputes `trusted` live against it)
    // AND registered as a KernelEventObserver below. Because the trust verdict
    // is recomputed at projection time, observer-registration order no longer
    // matters for correctness.
    let active_follow_set = ActiveFollowSet::new(app_ref.active_account_handle());
    // Inject the live follow set into SocialState so agent_notes_snapshot()
    // computes `trusted` at projection time (not frozen at receipt). Mirrors
    // the `voice` field-replacement pattern above: the fresh substate's slots
    // are the ones observers `.share()` from below (the default `social`
    // substate built in `new_with_identity` was never shared).
    app_state_inner.social =
        crate::state::social::SocialState::new(app_state_inner.social.infra.clone())
            .with_follow_set(Arc::clone(&active_follow_set));

    let app_state = Arc::new(app_state_inner);
    // Step 16: feed_fetch is now app_state.feed_fetch; no local variable needed.

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
    // SEED-IF-EMPTY — investigated, intentionally still unconditional.
    //
    // The `configured_relays` projection exists and `podcast.settings`
    // exposes add/remove/set_role ops. Relay edits are now persisted across
    // restarts via the `.nmp-relay-config.json` sidecar (commit 0dcf9680,
    // PR #220): `ffi/data_dir.rs` loads saved relays before `nmp_app_start`
    // and `ffi/relay_persist.rs` saves after each mutation. Despite that, the
    // seed here remains unconditional for one structural reason:
    //
    //   At `register` time the slot is ALWAYS empty. The actor only populates
    //   `configured_relays` from `initial_relays` when it handles
    //   `ActorCommand::Start`, which runs AFTER `register` returns. The load
    //   in `data_dir.rs` also runs before `Start` and calls
    //   `set_initial_relays_for_start` — if it finds saved relays, the
    //   persisted list wins (it is staged last and `Start` takes the final
    //   staged value). So on subsequent launches the persistence load
    //   effectively overrides this seed. On a truly fresh install the sidecar
    //   is absent and this seed provides the correct defaults.
    //
    // Net: this seed is correct and harmless — on fresh installs it provides
    // the default relay set; on restarts the persistence load overwrites it
    // with the user's saved configuration.
    app_ref.set_initial_relays_for_start(vec![
        (
            "wss://relay.primal.net".to_string(),
            "both,indexer".to_string(),
        ),
        ("wss://purplepag.es".to_string(), "indexer".to_string()),
        // In-app feedback source relay. Seeded read-only so NMP opens the
        // connection used by the relay-pinned feedback subscription; publish
        // targets the same relay explicitly through `nmp-feedback`.
        // Step 16: feedback is now in app_state.feedback.
        app_state.feedback.config().relay_seed(),
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
    // Step N+1: handler now takes only (app, state) — all infra is in state.infra.
    app_ref.set_host_op_handler(Arc::new(PodcastHostOpHandler::new(app, app_state.clone())));

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
    // Step N+1: observers use infra clones from app_state rather than separate locals.
    let _discovery_observer_id = app_ref.register_event_observer(std::sync::Arc::new(
        crate::discover_nostr::NostrDiscoveryObserver::new(
            app_state.discovery.nostr_results.share(),
            app_state.infra.rev.clone(),
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
            app_state.infra.rev.clone(),
        )
        .with_snapshot_signal(snapshot_signal.clone()),
    ));

    // ── Reactive social-graph observers ──────────────────────────────────────
    //
    // `active_follow_set` was constructed above (before sealing `app_state`)
    // and injected into SocialState. Here it is registered as a
    // KernelEventObserver so kind:3 events keep it current, and an
    // identity-change hook resets all per-account social state on switch.
    //
    // Account-change hook: `register_identity_change_observer` fires on the
    // update-listener thread whenever the active pubkey changes (sign-in /
    // switch / logout). It:
    //   1. `notify_account_changed` — clears the stale follow set and re-seeds
    //      the new account's own pubkey (self-inclusion).
    //   2. `clear_for_account_switch` — empties `social_slot` + `agent_notes`
    //      so A's following list and A's notes don't bleed into B's session
    //      (cross-account leak fix).
    {
        let afs = Arc::clone(&active_follow_set);
        let state_for_switch = app_state.clone();
        app_ref.register_identity_change_observer(move |_| {
            afs.notify_account_changed();
            state_for_switch.social.clear_for_account_switch();
        });
    }
    // Clone as the concrete type, then let the fn-arg position coerce
    // Arc<ActiveFollowSet> → Arc<dyn KernelEventObserver> (unsizing).
    let _follow_set_observer_id =
        app_ref.register_event_observer(active_follow_set.clone());

    // FollowListObserver: materialises a SocialSnapshot from the inner
    // FollowListProjection on every kind:3 push frame and writes it to
    // state.social.social_slot.  Uses the kernel's standing
    // account_profile_interest subscription (kind:0 + kind:3 + kind:10002) —
    // no extra relay subscription needed.
    let _follow_list_observer_id = app_ref.register_event_observer(std::sync::Arc::new(
        crate::social_handler::FollowListObserver::new(
            app_ref.active_account_handle(),
            app_state.social.social_slot.share(),
            app_state.infra.rev.clone(),
        )
        .with_snapshot_signal(snapshot_signal.clone()),
    ));

    // kind:1 agent-notes observer — receives events from push_interest_via_nmp
    // subscriptions opened by handle_fetch_agent_notes. No iOS WebSocket.
    // It caches raw notes (author hex retained, NO trust stamp); the trust
    // verdict is recomputed live at projection time in SocialState against the
    // shared ActiveFollowSet (so follow/unfollow flips existing notes).
    let _agent_notes_observer_id = app_ref.register_event_observer(std::sync::Arc::new(
        crate::agent_note_handler::AgentNotesObserver::new(
            identity.clone(),
            app_state.social.agent_notes.share(),
            app_state.infra.rev.clone(),
        )
        .with_snapshot_signal(snapshot_signal.clone()),
    ));

    // In-app feedback observer. The reusable module owns event filtering,
    // bounded caching, and snapshot rev bumps. Unlike agent-notes, it does NOT
    // self-filter — the Feedback UI shows the user's own threads.
    // Step 16: feedback is now in app_state.feedback.
    let _feedback_observer_id =
        app_ref.register_event_observer(std::sync::Arc::new(app_state.feedback.observer()));

    // Step N+1: PodcastHandle is now the minimal 2-field shell:
    //   app  — raw *mut NmpApp for capability dispatch
    //   state — Arc<PodcastAppState> (the entire tree)
    //   snapshot_cache + clean_html_cache — perf caches owned by the handle
    //
    // Everything else (rev, signal, runtime, store, identity, feedback,
    // feed_fetch, player, queue, …) is accessed via state.infra.* /
    // state.library.* / state.playback.* etc.
    let handle = Arc::new(PodcastHandle {
        app,
        state: app_state,
        snapshot_cache: Arc::new(Mutex::new(None)),
        clean_html_cache: Arc::new(Mutex::new(std::collections::HashMap::new())),
    });

    // Per-domain typed snapshot projections (perf/domain-sub-projections-kernel).
    //
    // Each closure runs on the actor thread on every tick (D8 — non-blocking).
    // It reads the domain's `Arc<AtomicU64>` rev and compares against its own
    // `last_emitted`; if unchanged, returns `None` so the sidecar is omitted
    // from the frame entirely — true push-side delta semantics.
    //
    // The Tier-3 encoder carries typed sidecars verbatim. The shell decodes them
    // via `nmp_app_podcast_decode_update_frame`, which injects every `podcast.*`
    // sidecar into `v.projections[key]` — available for future Swift/Android
    // consumption without touching the pull path.
    //
    // The PULL path (`nmp_app_podcast_snapshot` / `build_snapshot_payload`) is
    // kept intact as cold-start hydration + fallback.
    super::snapshot_domain_projections::register_domain_projections(app_ref, &handle);

    // Ownership: one strong ref is returned to the shell as the opaque handle
    // pointer; the projection closure above holds a second strong ref for the
    // app's lifetime. `nmp_app_podcast_unregister` reclaims the shell's ref via
    // `Arc::from_raw`. `nmp_app_free` joins the actor thread before dropping, so
    // no projector call is in flight after teardown. The handle is only ever
    // borrowed shared across the FFI (no `&mut`), so `Arc` aliasing is sound.
    Arc::into_raw(handle) as *mut PodcastHandle
    }) // ffi_guard
}
