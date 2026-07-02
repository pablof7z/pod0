//! The `pub extern "C"` registration entry point Swift links against to wire
//! Podcast projections and action namespaces into an [`NmpApp`].

use std::sync::{Arc, Mutex};

use crate::state::{Infra, PodcastAppState};

use nmp_native_runtime::NmpApp;
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
use super::guard::ffi_guard;
use super::handle::PodcastHandle;
use crate::host_op_handler::PodcastHostOpHandler;
use crate::store::agent_note_responder_cache::ResponderCache;
use crate::store::approved_peer_store::ApprovedPeerStore;
use crate::store::outbound_turn_cache::OutboundTurnCache;
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
    // ADR-0069 explicit composition (A1, pablof7z/podcast-player#681):
    // `nmp-defaults::register_defaults` is deleted upstream. This installs
    // the reusable substrate floor plus the same protocol action modules
    // register_defaults used to bundle (NIP-02 follow, NIP-18 repost/quote-
    // repost, NIP-25 react/unreact, NIP-17 DM send + relay-list + DM-inbox
    // runtime, NIP-65 publish-relay-list action + routing substrate, NIP-51
    // bookmarks, WOT bootstrap) — see the NMP repo's
    // `crates/nmp-cli/templates/lib.rs.tmpl` for the canonical shape this
    // mirrors.
    let _substrate_handles =
        nmp_substrate::install(app_mut, nmp_substrate::SubstrateConfig::default());
    nmp_nip02::register_follow_actions(app_mut);
    nmp_core::substrate::ProtocolDescriptor::register_actions(&nmp_nip18::Nip18Descriptor, app_mut);
    nmp_core::substrate::ProtocolDescriptor::register_actions(&nmp_nip25::Nip25Descriptor, app_mut);
    nmp_nip17::register_actions(app_mut);
    nmp_nip17::register_runtime(app_mut);
    nmp_router::register_actions(app_mut);
    let _bookmark_runtime = nmp_nip51::register_bookmark_runtime(app_mut);
    let _wot_runtime = nmp_wot::register_runtime(app_mut);

    // Wire the BUD-02 Blossom upload action (`nmp.blossom.upload`).
    // D13/D0: Rust owns the full Build → Sign → Transport pipeline.
    // Swift dispatches with a correlation-id and reads the BlobDescriptor
    // from action_results[correlation_id].result on the next push frame.
    nmp_blossom::register_actions(app_mut);

    // `register_action` now returns `Result<(), RegistrationError>` (an
    // app-over-app namespace collision, ADR-0049) instead of `()`. Every
    // namespace below is distinct and app-owned, so a collision here would
    // indicate a real bug; `expect` surfaces that loudly in dev AND release
    // builds rather than silently swallowing it (matches the trait doc's
    // guidance — `register_default_action` is the yielding variant for
    // deliberate overrides, not this one).
    app_mut.register_action(IdentityActionModule).expect("podcast.identity namespace collision");
    app_mut.register_action(PodcastActionModule).expect("podcast namespace collision");
    app_mut.register_action(PlayerActionModule).expect("podcast.player namespace collision");
    app_mut.register_action(QueueActionModule).expect("podcast.queue namespace collision");
    app_mut.register_action(ChaptersActionModule).expect("podcast.chapters namespace collision");
    app_mut.register_action(AgentPicksModule).expect("podcast.picks namespace collision");
    app_mut.register_action(AgentTasksModule).expect("podcast.tasks namespace collision");
    app_mut.register_action(KnowledgeActionModule).expect("podcast.knowledge namespace collision");
    app_mut.register_action(MemoryActionModule).expect("podcast.memory namespace collision");
    app_mut.register_action(ClipActionModule).expect("podcast.clip namespace collision");
    app_mut.register_action(InboxActionModule).expect("podcast.inbox namespace collision");
    app_mut.register_action(NipF4PublishModule).expect("podcast.publish namespace collision");
    app_mut.register_action(VoiceActionModule).expect("podcast.voice namespace collision");
    app_mut.register_action(AgentActionModule).expect("podcast.agent namespace collision");
    app_mut.register_action(CategorizationModule).expect("podcast.categorize namespace collision");
    app_mut.register_action(SettingsActionModule).expect("podcast.settings namespace collision");
    app_mut.register_action(SiriActionModule).expect("podcast.siri namespace collision");
    app_mut.register_action(SocialActionModule).expect("podcast.social namespace collision");

    // Shared state between the handle (snapshot reader) and the handler (writer).
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let identity = Arc::new(Mutex::new(IdentityStore::new()));
    // player_actor removed in Step 14 — now seeded inside PlaybackState.
    // search_results and nostr_results removed in Step 9 —
    // now owned by state.discovery (DiscoveryState).
    // queue removed in Step 14 — now seeded inside PlaybackState.
    // download_queue removed in Step 14 — now seeded inside PlaybackState.
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
    // Start at 1 so the first snapshot delivery always triggers an iOS update
    // (guard is `update.rev > last_seen_rev`; last_seen_rev starts at 0).
    let rev = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(1));
    let snapshot_signal = SnapshotUpdateSignal::new(rev.clone(), app_ref.actor_sender());

    // TODO(follow-up, pablof7z/nmp-feedback#3): the feedback runtime used to
    // be constructed here (needs the snapshot-bump hook that captures the
    // live signal) and injected into PodcastAppState. Dropped in A0/A1
    // (pablof7z/podcast-player#680/#681) — see the workspace Cargo.toml
    // pin-discipline comment. Restore once nmp-feedback ships a rewrite past
    // the deleted nmp-ffi JSON publish doorway.

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
    );
    // Step 12: Replace the default null-app VoiceSubstate with one that holds
    // the live `app` pointer so `VoiceConversationManager` can dispatch
    // `VoiceCommand::Speak` back to the iOS TTS executor.
    app_state_inner.voice = crate::state::voice::VoiceSubstate::new(
        app_state_infra.with_domain(crate::state::Domain::Voice),
        store.clone(),
        app,
    );

    // ── Kernel-owned task scheduler tick (D9 / D13) ───────────────────────────
    //
    // Spawn the periodic 60-second due-task check NOW — after the real `app`
    // pointer is available and before `app_state_inner` is sealed into `Arc`.
    // The ticker captures `app` (wrapped as a `usize` for Send) and runs for
    // the lifetime of the Tokio runtime (dropped with the handle).
    //
    // Slice 2 will delete the iOS / Android host foreground poll paths
    // (`AppStateStore.runDueScheduledTasksIfNeeded` / Android `TaskRunDuePayload`);
    // until then both paths are idempotent — whichever reaches `run_task_by_id`
    // first advances `next_run_at`, and the other finds no due tasks.
    app_state_inner.tasks.start_ticker(app);

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
    let active_follow_set = ActiveFollowSet::new(
        app_ref.active_account_handle(),
        nmp_nip02::LatestKind3FollowSet::new(app_ref.event_store_handle()),
    );

    // NOTE: outbound_turn_cache is constructed and seeded from disk below,
    // AFTER data_dir is bound. We keep a placeholder empty Arc here so the
    // social state has a slot to share immediately; the actual disk load
    // happens in data_dir.rs (nmp_app_podcast_set_data_dir). The Arc<Mutex>
    // is shared with AgentNotesObserver so the observer can persist turns.
    let outbound_turn_cache = Arc::new(std::sync::Mutex::new(OutboundTurnCache::new()));

    // Approved-peer store: kernel-owned approve/block allow-list.
    // Constructed empty here; seeded from disk in data_dir.rs after the data
    // dir is bound. The same Arc is injected into SocialState (trust predicate)
    // AND stored on PodcastHandle (so data_dir.rs can seed it after load).
    let approved_peer_store = Arc::new(std::sync::Mutex::new(ApprovedPeerStore::new()));

    // Inject the live follow set AND the approved-peer store into SocialState
    // so trust_predicate() computes `(followed || approved) && !blocked` at
    // projection time (not frozen at receipt). Mirrors the `voice`
    // field-replacement pattern above: the fresh substate's slots are the
    // ones observers `.share()` from below.
    app_state_inner.social =
        crate::state::social::SocialState::new(app_state_inner.social.infra.clone())
            .with_follow_set(Arc::clone(&active_follow_set))
            .with_approved_peers(Arc::clone(&approved_peer_store));

    let app_state = Arc::new(app_state_inner);

    // ── Agent-note auto-responder cache ──────────────────────────────────────
    //
    // Shared between the handle (where `data_dir.rs` restores persisted state)
    // and the `AgentNotesObserver` (which checks + updates it on each reply).
    // The `Arc` is cheap to clone; the inner `Mutex` protects access between
    // the observer thread and the data_dir restore call.
    let responder_cache = Arc::new(Mutex::new(ResponderCache::default()));
    // Outbound-turn cache is also shared between the handle (data_dir.rs seeding)
    // and the AgentNotesObserver (append after publish). The social state's
    // outbound_turns slot is shared via .share() so the in-memory projection
    // sees new turns immediately. The disk cache keeps turns across restarts.
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
        // TODO(follow-up, pablof7z/nmp-feedback#3): this list used to also
        // seed the in-app feedback source relay read-only
        // (`app_state.feedback.config().relay_seed()`) so NMP opened the
        // connection used by the relay-pinned feedback subscription.
        // Dropped with the feedback runtime in A0/A1.
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

    // TODO(A1 STUB, pablof7z/podcast-player#690): the reactive observer
    // registrations below (NIP-F4 discovery, kind:1111 comments,
    // ActiveFollowSet kind:3 reactivity, FollowListObserver, kind:1
    // agent-notes, and the dropped feedback observer) are disabled here.
    // `NmpApp::register_event_observer` (blanket, filterless) is deleted
    // upstream — the replacement is a DECLARATIVE `ObservedProjection`
    // (filter shape + consumer_id + scope) opened through
    // `NmpApp::observed_projection_handle()`. Re-expressing each observer
    // this way (plus, for discovery specifically, redesigning the
    // release/resubscribe id-tracking around `ObservedProjectionId`) is
    // tracked in #690, not mechanical enough for A1's compile-only scope.
    // Until that lands: discovery/comments/social/agent-notes subscriptions
    // can still be OPENED (see `push_interest_via_nmp` call sites), but nothing
    // pushes the resulting events into the corresponding snapshot slots —
    // this is a real, tracked functional gap, not a silent no-op.
    //
    // Identity-change handling (clearing per-account social state on
    // sign-in/switch/logout) is independent of the observer registration
    // above and still wired below.
    {
        let afs = Arc::clone(&active_follow_set);
        let state_for_switch = app_state.clone();
        app_ref.register_identity_change_observer(move |_| {
            afs.notify_account_changed();
            state_for_switch.social.clear_for_account_switch();
        });
    }

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
        responder_cache,
        outbound_turn_cache,
        approved_peer_store,
        ask_state: Arc::new(Mutex::new(super::agent_ask::AgentAskState::default())),
        ask_callback: Arc::new(Mutex::new(super::agent_ask::AgentAskCallbackState::default())),
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
