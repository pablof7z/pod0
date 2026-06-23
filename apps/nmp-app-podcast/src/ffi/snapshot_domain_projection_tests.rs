//! Tests for per-domain typed snapshot projections.
//!
//! Key assertions per the task spec:
//!  - Frame round-trips: each domain closure emits valid `TypedProjectionData`
//!  - Delta proof: a playback-rev bump emits ONLY the `podcast.playback` sidecar
//!    (library, settings, identity, widget, misc closures return `None`)
//!  - Decoder: `decode_podcast_domain_sidecars` correctly filters and parses
//!  - `last_emitted` guard: a second call with the same domain rev → `None`
//!  - `DomainRevs::new()` starts all counters at 1
//!  - `infra.bump_domain` advances both the domain rev and the global rev

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use nmp_core::substrate::KernelEvent;
use nmp_core::KernelEventObserver;
use nmp_core::{encode_snapshot_frame, SnapshotEnvelope, TypedProjectionData};

use crate::agent_note_handler::AgentNotesObserver;
use crate::ffi::handle::PodcastHandle;
use crate::ffi::snapshot_domain_projections::{
    decode_podcast_domain_sidecars, register_domain_projections, SCHEMA_DOWNLOADS,
    SCHEMA_IDENTITY, SCHEMA_LIBRARY, SCHEMA_MISC, SCHEMA_PLAYBACK, SCHEMA_SETTINGS,
    SCHEMA_SOCIAL, SCHEMA_WIDGET,
};
use crate::state::{Domain, DomainRevs, Infra, PodcastAppState};
use crate::store::PodcastStore;

// ── Test helpers ──────────────────────────────────────────────────────────────

/// Make a handle with a real (unstarted) `NmpApp` so `build_configured_relays`
/// does not deref a null pointer. The caller is responsible for freeing `app`
/// after dropping the handle.
fn make_test_handle_with_app(app: *mut nmp_ffi::NmpApp) -> Box<PodcastHandle> {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let state = Arc::new(PodcastAppState::new(
        Infra::for_test(),
        store.clone(),
    ));
    // Clear agent_tasks (default seed uses Uuid::new_v4 — non-deterministic).
    state.tasks.tasks.lock().unwrap().clear();

    Box::new(PodcastHandle {
        app,
        state,
        responder_cache: Arc::new(Mutex::new(crate::store::agent_note_responder_cache::ResponderCache::default())),
        outbound_turn_cache: Arc::new(Mutex::new(crate::store::outbound_turn_cache::OutboundTurnCache::new())),
        approved_peer_store: Arc::new(Mutex::new(crate::store::approved_peer_store::ApprovedPeerStore::new())),
        snapshot_cache: Arc::new(Mutex::new(None)),
        clean_html_cache: Arc::new(Mutex::new(HashMap::new())),
        ask_state: Arc::new(Mutex::new(crate::ffi::agent_ask::AgentAskState::default())),
        ask_callback: Arc::new(Mutex::new(crate::ffi::agent_ask::AgentAskCallbackState::default())),
    })
}

/// Build a handle whose `state.social` has an approved-peer store wired (the
/// production `register.rs` shape), plus return the shared `Arc<PodcastAppState>`
/// and the shared `ApprovedPeerStore` arc so a `PodcastHostOpHandler` can be
/// constructed over the SAME state and the test can drive real social actions.
///
/// `make_test_handle_with_app` leaves `social.approved == None` (the default
/// `PodcastAppState::new`), which is fine for inbound-note tests but cannot
/// exercise the approve/block action seam. This helper replaces `state.social`
/// the same way `register.rs` does — before the `Arc` wrap — so the action
/// handler, the trust predicate, and the projection all read one store.
fn make_handle_and_state_with_approved(
    app: *mut nmp_ffi::NmpApp,
) -> (
    Arc<PodcastHandle>,
    Arc<PodcastAppState>,
    Arc<Mutex<crate::store::approved_peer_store::ApprovedPeerStore>>,
) {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let mut state_inner = PodcastAppState::new(Infra::for_test(), store.clone());
    state_inner.tasks.tasks.lock().unwrap().clear();

    let approved = Arc::new(Mutex::new(
        crate::store::approved_peer_store::ApprovedPeerStore::new(),
    ));
    // Replace social with one wired to the shared approved store (register.rs shape).
    state_inner.social = crate::state::social::SocialState::new(state_inner.social.infra.clone())
        .with_approved_peers(Arc::clone(&approved));

    let state = Arc::new(state_inner);

    let handle = Arc::new(PodcastHandle {
        app,
        state: Arc::clone(&state),
        responder_cache: Arc::new(Mutex::new(
            crate::store::agent_note_responder_cache::ResponderCache::default(),
        )),
        outbound_turn_cache: Arc::new(Mutex::new(
            crate::store::outbound_turn_cache::OutboundTurnCache::new(),
        )),
        approved_peer_store: Arc::clone(&approved),
        snapshot_cache: Arc::new(Mutex::new(None)),
        clean_html_cache: Arc::new(Mutex::new(HashMap::new())),
        ask_state: Arc::new(Mutex::new(crate::ffi::agent_ask::AgentAskState::default())),
        ask_callback: Arc::new(Mutex::new(crate::ffi::agent_ask::AgentAskCallbackState::default())),
    });

    (handle, state, approved)
}

fn make_frame_with_sidecars(sidecars: &[TypedProjectionData]) -> Vec<u8> {
    let env = SnapshotEnvelope {
        rev: 1,
        running: true,
        ..SnapshotEnvelope::default()
    };
    encode_snapshot_frame(&env, sidecars)
}

/// Run typed snapshot projections and filter out the always-emitting
/// `claimed_event_embeds` sidecar registered by nmp-ffi's embed sidecar
/// (ac7e307e: `install_embed_sidecar_projection` always returns `Some`).
/// Domain-projection tests assert silence between domain-rev bumps; the
/// embed sidecar's unconditional emit would cause false failures.
fn run_domain_projections_only(app_ref: &nmp_ffi::NmpApp) -> Vec<TypedProjectionData> {
    app_ref
        .run_typed_snapshot_projections()
        .into_iter()
        .filter(|p| p.schema_id != "claimed_event_embeds")
        .collect()
}

/// Build a REAL `AgentNotesObserver` wired to the handle's social state — the
/// SAME `agent_notes` cache the social projection reads AND the SAME
/// `Domain::Social`-scoped `Infra` whose `bump()` advances `domain_revs.social`.
///
/// This is the production wiring (minus the auto-responder); driving a kernel
/// event through it exercises the exact mutation→bump→re-emit path, so the test
/// fails if a future edit stops bumping the social domain rev. NO manual
/// `fetch_add` — that would mask the very bug this guards.
fn make_social_observer(handle: &Arc<PodcastHandle>) -> AgentNotesObserver {
    // Empty identity → `my_pubkey` is empty → inbound peer notes are NOT
    // dropped as self-authored.
    let identity = Arc::new(Mutex::new(crate::store::identity::IdentityStore::new()));
    AgentNotesObserver::new(
        identity,
        handle.state.social.agent_notes.share(),
        Arc::clone(&handle.state.infra.rev),
    )
    .with_social_infra(handle.state.social.infra.clone())
}

/// A minimal inbound kind:1 NIP-01 text note from `author_hex`.
fn inbound_note(id: &str, author_hex: &str) -> KernelEvent {
    KernelEvent {
        id: id.to_string(),
        author: author_hex.to_string(),
        kind: 1,
        created_at: 1_717_200_000,
        tags: Vec::new(),
        content: "hello from a followed peer".to_string(),
        relay_provenance: vec![],
    }
}

// ── DomainRevs construction ───────────────────────────────────────────────────

/// `DomainRevs::new` starts all counters at 1 so the first emit always fires.
#[test]
fn domain_revs_start_at_one() {
    let dr = DomainRevs::new();
    assert_eq!(dr.library.load(Ordering::Relaxed), 1);
    assert_eq!(dr.playback.load(Ordering::Relaxed), 1);
    assert_eq!(dr.downloads.load(Ordering::Relaxed), 1);
    assert_eq!(dr.settings.load(Ordering::Relaxed), 1);
    assert_eq!(dr.identity.load(Ordering::Relaxed), 1);
    assert_eq!(dr.widget.load(Ordering::Relaxed), 1);
    assert_eq!(dr.social.load(Ordering::Relaxed), 1);
    assert_eq!(dr.misc.load(Ordering::Relaxed), 1);
}

/// `infra.bump_domain_explicit` advances both the named domain rev and the
/// global rev.
#[test]
fn bump_domain_explicit_advances_both_revs() {
    let infra = Infra::for_test();
    let initial_global = infra.rev.load(Ordering::Relaxed);
    let initial_domain = infra.domain_revs.library.load(Ordering::Relaxed);

    infra.bump_domain_explicit(Domain::Library);

    assert_eq!(
        infra.domain_revs.library.load(Ordering::Relaxed),
        initial_domain + 1,
        "named domain rev must have incremented by 1"
    );
    assert!(
        infra.rev.load(Ordering::Relaxed) > initial_global,
        "global rev must also advance after bump_domain_explicit"
    );
}

/// A `Domain`-scoped `Infra`'s bare `bump()` routes to that domain's rev.
#[test]
fn scoped_bump_routes_to_domain_rev() {
    let infra = Infra::for_test().with_domain(Domain::Playback);
    let initial_playback = infra.domain_revs.playback.load(Ordering::Relaxed);
    let initial_library = infra.domain_revs.library.load(Ordering::Relaxed);

    infra.bump();

    assert_eq!(
        infra.domain_revs.playback.load(Ordering::Relaxed),
        initial_playback + 1,
        "scoped bump() must advance the playback domain rev"
    );
    assert_eq!(
        infra.domain_revs.library.load(Ordering::Relaxed),
        initial_library,
        "scoped bump() must NOT advance an unrelated domain rev"
    );
}

// ── Decoder: decode_podcast_domain_sidecars ───────────────────────────────────

/// A frame with no `podcast.*` sidecars yields `None` (D6 — degrade silently).
#[test]
fn decode_absent_sidecars_yields_none() {
    let frame = make_frame_with_sidecars(&[]);
    assert!(
        decode_podcast_domain_sidecars(&frame).is_none(),
        "frame without podcast.* sidecars must yield None"
    );
}

/// A frame with a `podcast.playback` sidecar carrying valid JSON is decoded
/// into a map entry keyed by `"podcast.playback"`.
#[test]
fn decode_podcast_playback_sidecar_is_extracted() {
    let payload = serde_json::json!({ "rev": 42u64, "now_playing": null, "queue": [] });
    let payload_bytes = serde_json::to_vec(&payload).unwrap();
    let sidecar = TypedProjectionData {
        key: SCHEMA_PLAYBACK.to_string(),
        schema_id: SCHEMA_PLAYBACK.to_string(),
        schema_version: 1,
        file_identifier: String::new(),
        payload: payload_bytes,
        ..Default::default()
    };
    let frame = make_frame_with_sidecars(&[sidecar]);
    let map = decode_podcast_domain_sidecars(&frame)
        .expect("frame with podcast.playback sidecar must yield Some");
    assert!(
        map.contains_key(SCHEMA_PLAYBACK),
        "decoded map must contain podcast.playback; keys: {:?}",
        map.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        map[SCHEMA_PLAYBACK]["rev"],
        serde_json::json!(42u64),
        "decoded rev must match payload"
    );
}

/// Non-`podcast.*` sidecars are not included in the decoded map.
#[test]
fn decode_ignores_non_podcast_sidecars() {
    let other_payload = serde_json::to_vec(&serde_json::json!({ "data": "irrelevant" })).unwrap();
    let other_sidecar = TypedProjectionData {
        key: "signed_events".to_string(),
        schema_id: "nmp.signedEvents".to_string(),
        schema_version: 1,
        file_identifier: "KSEV".to_string(),
        payload: other_payload,
        ..Default::default()
    };
    let frame = make_frame_with_sidecars(&[other_sidecar]);
    assert!(
        decode_podcast_domain_sidecars(&frame).is_none(),
        "non-podcast.* sidecars must not appear in the podcast domain decoder output"
    );
}

/// A sidecar with malformed (non-JSON) payload is silently skipped (D6).
#[test]
fn decode_malformed_sidecar_payload_is_silently_skipped() {
    let bad = TypedProjectionData {
        key: SCHEMA_LIBRARY.to_string(),
        schema_id: SCHEMA_LIBRARY.to_string(),
        schema_version: 1,
        file_identifier: String::new(),
        payload: b"not json {{{".to_vec(),
        ..Default::default()
    };
    let frame = make_frame_with_sidecars(&[bad]);
    // The single sidecar has a bad payload; the map ends up empty → None (D6).
    assert!(
        decode_podcast_domain_sidecars(&frame).is_none(),
        "malformed podcast.* sidecar payload must be silently skipped (D6)"
    );
}

// ── Delta proof: playback bump excludes library ───────────────────────────────

/// Core delta assertion from the task spec:
/// "playback-tick frame EXCLUDES the library sidecar".
///
/// When only `domain_revs.playback` is bumped (simulating a playback tick),
/// the library closure's `last_emitted` matches `domain_revs.library`, so
/// the library sidecar is absent from the frame.
#[test]
fn playback_tick_excludes_library_sidecar() {
    let app = nmp_ffi::nmp_app_new();
    assert!(!app.is_null(), "nmp_app_new must succeed");
    let app_ref = unsafe { &*app };

    let handle = Arc::new(*make_test_handle_with_app(app));
    let domain_revs = Arc::clone(&handle.state.infra.domain_revs);

    register_domain_projections(app_ref, &handle);

    // First call: all domain revs start at 1, all last_emitted start at 0 →
    // most closures fire. Run to consume the initial state.
    let _ = app_ref.run_typed_snapshot_projections();

    // Second call without any rev bump → ALL closures return None (no change).
    let no_change = run_domain_projections_only(app_ref);
    assert!(
        no_change.is_empty(),
        "second run with no domain rev bump must emit nothing (all closures return None); got {:?}",
        no_change.iter().map(|p| p.schema_id.as_str()).collect::<Vec<_>>()
    );

    // Bump only the playback domain rev.
    domain_revs.playback.fetch_add(1, Ordering::Relaxed);

    // Third call → only podcast.playback is emitted; library is absent.
    let after_playback_bump = app_ref.run_typed_snapshot_projections();
    let keys_after: Vec<&str> = after_playback_bump
        .iter()
        .map(|p| p.schema_id.as_str())
        .collect();

    assert!(
        keys_after.contains(&SCHEMA_PLAYBACK),
        "podcast.playback must be emitted after playback domain bump; got {keys_after:?}"
    );
    assert!(
        !keys_after.contains(&SCHEMA_LIBRARY),
        "podcast.library must NOT be in the frame after a playback-only bump (delta proof); got {keys_after:?}"
    );
    assert!(
        !keys_after.contains(&SCHEMA_SETTINGS),
        "podcast.settings must NOT be in the frame after a playback-only bump; got {keys_after:?}"
    );

    drop(handle);
    unsafe { drop(Box::from_raw(app)) };
}

// ── Per-domain round-trip: sidecars carry valid JSON ─────────────────────────

/// Each domain sidecar (when emitted) carries a JSON payload that includes
/// a `rev` field and the domain-specific data keys.
#[test]
fn domain_projections_emit_valid_json_with_rev_field() {
    let app = nmp_ffi::nmp_app_new();
    assert!(!app.is_null(), "nmp_app_new must succeed");
    let app_ref = unsafe { &*app };

    let handle = Arc::new(*make_test_handle_with_app(app));
    register_domain_projections(app_ref, &handle);

    // First call emits everything (all domain revs start at 1, last_emitted at 0).
    let projections = app_ref.run_typed_snapshot_projections();

    // With the tombstone contract, downloads/identity/widget always emit on first
    // run (tombstone if empty, full payload if populated). settings, playback,
    // library, and misc must also be present.
    // Filter claimed_event_embeds: it's an nmp-ffi sidecar (ac7e307e) that emits
    // FlatBuffer bytes (not JSON), so including it in the JSON-validity loop below
    // would fail. Domain-projection tests only care about podcast.* sidecars.
    let by_key: std::collections::HashMap<String, &TypedProjectionData> = projections
        .iter()
        .filter(|p| p.schema_id != "claimed_event_embeds")
        .map(|p| (p.schema_id.clone(), p))
        .collect();

    for (key, entry) in &by_key {
        let value: serde_json::Value = serde_json::from_slice(&entry.payload)
            .unwrap_or_else(|e| panic!("domain {key} sidecar must be valid JSON: {e}"));
        assert!(
            value.get("rev").is_some(),
            "domain {key} payload must carry a 'rev' field"
        );
    }

    // settings must always be present (non-optional payload).
    assert!(
        by_key.contains_key(SCHEMA_SETTINGS),
        "podcast.settings must be emitted on initial run; got: {:?}",
        by_key.keys().collect::<Vec<_>>()
    );
    // playback must always be present.
    assert!(
        by_key.contains_key(SCHEMA_PLAYBACK),
        "podcast.playback must be emitted on initial run; got: {:?}",
        by_key.keys().collect::<Vec<_>>()
    );
    // misc must always be present.
    assert!(
        by_key.contains_key(SCHEMA_MISC),
        "podcast.misc must be emitted on initial run; got: {:?}",
        by_key.keys().collect::<Vec<_>>()
    );

    drop(handle);
    unsafe { drop(Box::from_raw(app)) };
}

// ── Tombstone contract ────────────────────────────────────────────────────────
//
// For each domain whose builder returns Option<Value>, verify:
//  1. changed→empty emits a tombstone (rev + nulled field).
//  2. A second tick with the same empty state returns None (no perpetual rebuild).

/// `podcast.library` empty → tombstone on first run (store is empty by default
/// in `make_test_handle_with_app`), then idles on a second tick.
#[test]
fn library_empty_emits_tombstone_then_idles() {
    let app = nmp_ffi::nmp_app_new();
    assert!(!app.is_null());
    let app_ref = unsafe { &*app };
    let handle = Arc::new(*make_test_handle_with_app(app));
    register_domain_projections(app_ref, &handle);

    // First run: rev 1 > last_emitted 0; library is empty → tombstone.
    let first = app_ref.run_typed_snapshot_projections();
    let lib = first.iter().find(|p| p.schema_id == SCHEMA_LIBRARY)
        .expect("library tombstone must be emitted when store is empty");
    let val: serde_json::Value = serde_json::from_slice(&lib.payload).unwrap();
    assert_eq!(val["library"], serde_json::Value::Null, "tombstone must carry library: null");
    assert!(val["rev"].is_number(), "tombstone must carry a rev number");

    // Second tick — last_emitted caught up → no library sidecar (no perpetual rebuild).
    let second = app_ref.run_typed_snapshot_projections();
    assert!(
        second.iter().all(|p| p.schema_id != SCHEMA_LIBRARY),
        "second empty tick must NOT emit library sidecar"
    );

    drop(handle);
    unsafe { drop(Box::from_raw(app)) };
}

/// `podcast.downloads` changed→empty emits tombstone, second empty tick is silent.
#[test]
fn downloads_empty_emits_tombstone_then_idles() {
    let app = nmp_ffi::nmp_app_new();
    assert!(!app.is_null());
    let app_ref = unsafe { &*app };
    let handle = Arc::new(*make_test_handle_with_app(app));
    let domain_revs = Arc::clone(&handle.state.infra.domain_revs);
    register_domain_projections(app_ref, &handle);

    // Consume initial run; ensure silence before the targeted bump.
    let _ = app_ref.run_typed_snapshot_projections();
    assert!(run_domain_projections_only(app_ref).is_empty());

    // Bump downloads rev; no active downloads in test store.
    domain_revs.downloads.fetch_add(1, Ordering::Relaxed);
    let after = app_ref.run_typed_snapshot_projections();
    let dl = after.iter().find(|p| p.schema_id == SCHEMA_DOWNLOADS)
        .expect("downloads tombstone must be emitted");
    let val: serde_json::Value = serde_json::from_slice(&dl.payload).unwrap();
    assert_eq!(val["downloads"], serde_json::Value::Null, "tombstone must carry downloads: null");

    // Next tick must be silent.
    let idle = app_ref.run_typed_snapshot_projections();
    assert!(idle.iter().all(|p| p.schema_id != SCHEMA_DOWNLOADS), "second empty tick must be silent");

    drop(handle);
    unsafe { drop(Box::from_raw(app)) };
}

/// `podcast.identity` changed→empty (no active account) emits tombstone, then idles.
#[test]
fn identity_empty_emits_tombstone_then_idles() {
    let app = nmp_ffi::nmp_app_new();
    assert!(!app.is_null());
    let app_ref = unsafe { &*app };
    let handle = Arc::new(*make_test_handle_with_app(app));
    let domain_revs = Arc::clone(&handle.state.infra.domain_revs);
    register_domain_projections(app_ref, &handle);

    let _ = app_ref.run_typed_snapshot_projections();
    assert!(run_domain_projections_only(app_ref).is_empty());

    domain_revs.identity.fetch_add(1, Ordering::Relaxed);
    let after = app_ref.run_typed_snapshot_projections();
    let ident = after.iter().find(|p| p.schema_id == SCHEMA_IDENTITY)
        .expect("identity tombstone must be emitted when no account is active");
    let val: serde_json::Value = serde_json::from_slice(&ident.payload).unwrap();
    assert_eq!(val["active_account"], serde_json::Value::Null, "tombstone must carry active_account: null");

    let idle = app_ref.run_typed_snapshot_projections();
    assert!(idle.iter().all(|p| p.schema_id != SCHEMA_IDENTITY), "second empty tick must be silent");

    drop(handle);
    unsafe { drop(Box::from_raw(app)) };
}

/// `podcast.widget` changed→empty (no playback, no episodes) emits tombstone, then idles.
#[test]
fn widget_empty_emits_tombstone_then_idles() {
    let app = nmp_ffi::nmp_app_new();
    assert!(!app.is_null());
    let app_ref = unsafe { &*app };
    let handle = Arc::new(*make_test_handle_with_app(app));
    let domain_revs = Arc::clone(&handle.state.infra.domain_revs);
    register_domain_projections(app_ref, &handle);

    let _ = app_ref.run_typed_snapshot_projections();
    assert!(run_domain_projections_only(app_ref).is_empty());

    domain_revs.widget.fetch_add(1, Ordering::Relaxed);
    let after = app_ref.run_typed_snapshot_projections();
    let wgt = after.iter().find(|p| p.schema_id == SCHEMA_WIDGET)
        .expect("widget tombstone must be emitted when widget is None");
    let val: serde_json::Value = serde_json::from_slice(&wgt.payload).unwrap();
    assert_eq!(val["widget"], serde_json::Value::Null, "tombstone must carry widget: null");

    let idle = app_ref.run_typed_snapshot_projections();
    assert!(idle.iter().all(|p| p.schema_id != SCHEMA_WIDGET), "second empty tick must be silent");

    drop(handle);
    unsafe { drop(Box::from_raw(app)) };
}

/// PR #417 propagation defect regression: an external NIP-55 / Amber sign-in
/// lands the active account by writing the kernel's active-account slot (the
/// V-82 single source of truth) WITHOUT advancing the app-owned
/// `domain_revs.identity` counter. The `podcast.identity` projection MUST
/// surface that account on the next emit — gating on the rev counter alone
/// dropped the sidecar from the very frame the kernel emitted for the
/// account-change, leaving the host on "Not signed in" after a successful
/// sign-in (flaky because an unrelated identity mutation occasionally bumped
/// the rev and dragged the fresh slot along). Here we write the slot directly
/// (the kernel's `set_accounts` effect) with the identity rev held fixed and
/// assert the identity sidecar is emitted with the Amber pubkey + `nip55` mode.
#[test]
fn identity_surfaces_kernel_active_account_without_rev_bump() {
    // The Amber test key proven on the emulator (PR #417 evidence).
    const AMBER_PUBKEY_HEX: &str =
        "d6070609432b666c51677f606a0961e5f40730fe44b1c3bbd7ce29d5fa25b0a6";
    const AMBER_NPUB: &str = "npub16crsvz2r9dnxc5t80asx5ztpuh6qwv87gjcu8w7hec5at739kznqzxadlu";

    let app = nmp_ffi::nmp_app_new();
    assert!(!app.is_null());
    let app_ref = unsafe { &*app };
    let handle = Arc::new(*make_test_handle_with_app(app));
    let domain_revs = Arc::clone(&handle.state.infra.domain_revs);
    register_domain_projections(app_ref, &handle);

    // Prime: first emit drains the initial sidecars; the next tick is silent.
    let _ = app_ref.run_typed_snapshot_projections();
    assert!(run_domain_projections_only(app_ref).is_empty());

    let rev_before = domain_revs.identity.load(Ordering::Relaxed);

    // Simulate the kernel landing the Amber account: write the V-82
    // active-account slot the kernel's `set_accounts` writes, WITHOUT bumping
    // `domain_revs.identity` (the NIP-55 path never touches the local store).
    {
        let slot = app_ref.active_account_handle();
        *slot.lock().unwrap() = Some(AMBER_PUBKEY_HEX.to_string());
    }

    // The identity sidecar MUST now be emitted, carrying the Amber account —
    // driven purely by the kernel slot transition, with the rev unchanged.
    let after = app_ref.run_typed_snapshot_projections();
    let ident = after.iter().find(|p| p.schema_id == SCHEMA_IDENTITY)
        .expect("identity sidecar must surface the kernel active account without a rev bump");
    let val: serde_json::Value = serde_json::from_slice(&ident.payload).unwrap();
    assert_eq!(
        val["active_account"]["pubkey_hex"], AMBER_PUBKEY_HEX,
        "must carry the kernel-active Amber pubkey"
    );
    assert_eq!(
        val["active_account"]["npub"], AMBER_NPUB,
        "npub must be derived from the kernel-active hex"
    );
    assert_eq!(
        val["active_account"]["mode"], "nip55",
        "external signer with no matching local key renders as nip55"
    );
    assert_eq!(
        domain_revs.identity.load(Ordering::Relaxed),
        rev_before,
        "fix must NOT depend on the identity rev advancing"
    );

    // Idempotence: a steady kernel slot must not re-emit every tick.
    let idle = app_ref.run_typed_snapshot_projections();
    assert!(idle.iter().all(|p| p.schema_id != SCHEMA_IDENTITY), "a steady kernel slot must not re-emit the identity sidecar");
    drop(handle);
    unsafe { drop(Box::from_raw(app)) };
}

/// `podcast.social` empty → tombstone on first run, then idles on second tick.
///
/// When the social state is empty (no account → no follow graph, no notes, no
/// conversations), `build_social_payload` returns `None` and the registration
/// closure emits `social_tombstone(rev)` so the iOS/Android decoders know to
/// clear their slice. A second tick with the same empty state returns `None`
/// (no perpetual rebuild).
#[test]
fn social_empty_emits_tombstone_then_idles() {
    let app = nmp_ffi::nmp_app_new();
    assert!(!app.is_null());
    let app_ref = unsafe { &*app };
    let handle = Arc::new(*make_test_handle_with_app(app));
    register_domain_projections(app_ref, &handle);

    // First run: rev 1 > last_emitted 0; no account → tombstone.
    let first = app_ref.run_typed_snapshot_projections();
    let soc = first.iter().find(|p| p.schema_id == SCHEMA_SOCIAL)
        .expect("social tombstone must be emitted when state is empty");
    let val: serde_json::Value = serde_json::from_slice(&soc.payload).unwrap();
    assert_eq!(
        val["social"], serde_json::Value::Null,
        "tombstone must carry social: null; got: {val}"
    );
    assert!(val["rev"].is_number(), "tombstone must carry a rev field");

    // Second tick — last_emitted caught up → no social sidecar.
    let second = app_ref.run_typed_snapshot_projections();
    assert!(
        second.iter().all(|p| p.schema_id != SCHEMA_SOCIAL),
        "second empty tick must NOT emit social sidecar"
    );

    // Drive a REAL inbound note through a production-wired observer (NOT a
    // manual fetch_add). The observer's `bump_social()` must advance
    // `domain_revs.social` so the sidecar re-emits — now carrying the note.
    let observer = make_social_observer(&handle);
    observer.on_kernel_event(&inbound_note(
        "evt-empty-then-note",
        "aabbccddeeff00112233445566778899aabbccddeeff001122334455667788aa",
    ));

    let after_note = app_ref.run_typed_snapshot_projections();
    let soc2 = after_note
        .iter()
        .find(|p| p.schema_id == SCHEMA_SOCIAL)
        .expect("social sidecar must RE-EMIT after a real inbound note (production bumped domain_revs.social)");
    let val2: serde_json::Value = serde_json::from_slice(&soc2.payload).unwrap();
    // No longer a tombstone — the note populated nostr_conversations.
    assert!(
        val2["nostr_conversations"].as_array().map(|a| !a.is_empty()).unwrap_or(false),
        "re-emitted social payload must carry the inbound conversation; got: {val2}"
    );

    drop(handle);
    unsafe { drop(Box::from_raw(app)) };
}

/// Delta isolation: a REAL inbound note (driven through the production observer)
/// emits ONLY `podcast.social`.
///
/// Mirrors the `playback_tick_excludes_library_sidecar` test but for the new
/// `podcast.social` 8th domain, and uses the production mutation path (NOT a
/// manual `fetch_add`) so it doubles as a guard that the inbound bump targets
/// the social domain rev and nothing else.
#[test]
fn social_inbound_note_excludes_library_and_playback_sidecars() {
    let app = nmp_ffi::nmp_app_new();
    assert!(!app.is_null());
    let app_ref = unsafe { &*app };

    let handle = Arc::new(*make_test_handle_with_app(app));

    register_domain_projections(app_ref, &handle);

    // Consume initial state (all domains fire once).
    let _ = app_ref.run_typed_snapshot_projections();
    // Second run with no bumps → all closures return None.
    let no_change = run_domain_projections_only(app_ref);
    assert!(
        no_change.is_empty(),
        "second run with no bump must emit nothing; got {:?}",
        no_change.iter().map(|p| p.schema_id.as_str()).collect::<Vec<_>>()
    );

    // Drive a REAL inbound note — production code bumps ONLY domain_revs.social.
    let observer = make_social_observer(&handle);
    observer.on_kernel_event(&inbound_note(
        "evt-delta-iso",
        "1111111111111111111111111111111111111111111111111111111111111111",
    ));

    let after = app_ref.run_typed_snapshot_projections();
    let keys: Vec<&str> = after.iter().map(|p| p.schema_id.as_str()).collect();

    assert!(
        keys.contains(&SCHEMA_SOCIAL),
        "podcast.social must be emitted after an inbound note; got {keys:?}"
    );
    assert!(
        !keys.contains(&SCHEMA_LIBRARY),
        "podcast.library must NOT be emitted after a social-only mutation (delta isolation); got {keys:?}"
    );
    assert!(
        !keys.contains(&SCHEMA_PLAYBACK),
        "podcast.playback must NOT be emitted after a social-only mutation; got {keys:?}"
    );
    assert!(
        !keys.contains(&SCHEMA_MISC),
        "podcast.misc must NOT be emitted after a social-only mutation; got {keys:?}"
    );

    drop(handle);
    unsafe { drop(Box::from_raw(app)) };
}

/// REGRESSION GUARD (the BLOCKER this PR was rejected for): a second inbound
/// note must RE-EMIT the social sidecar.
///
/// The original bug: the social projection gates on `domain_revs.social`, but
/// the inbound mutation path bumped only the GLOBAL signal — so the sidecar
/// emitted once (current 1 > last_emitted 0) and then idled forever
/// (current == prev), and new notes never reached iOS/Android. This test drives
/// TWO real inbound notes through the production observer and asserts the
/// SECOND one also re-emits `podcast.social` (proving production code advances
/// the social domain rev on every mutation, not just the first).
#[test]
fn social_inbound_note_reemits_on_each_new_note_real_path() {
    let app = nmp_ffi::nmp_app_new();
    assert!(!app.is_null());
    let app_ref = unsafe { &*app };

    let handle = Arc::new(*make_test_handle_with_app(app));
    register_domain_projections(app_ref, &handle);

    let observer = make_social_observer(&handle);

    // Consume the initial tombstone tick, then confirm silence.
    let _ = app_ref.run_typed_snapshot_projections();
    assert!(
        run_domain_projections_only(app_ref).is_empty(),
        "no social sidecar should emit before any note arrives"
    );

    // First inbound note → sidecar re-emits.
    observer.on_kernel_event(&inbound_note(
        "evt-real-1",
        "2222222222222222222222222222222222222222222222222222222222222222",
    ));
    let after_first = app_ref.run_typed_snapshot_projections();
    assert!(
        after_first.iter().any(|p| p.schema_id == SCHEMA_SOCIAL),
        "first inbound note must re-emit podcast.social"
    );
    // Idle confirms last_emitted caught up.
    assert!(
        run_domain_projections_only(app_ref).is_empty(),
        "social sidecar must idle after the first note is emitted"
    );

    // SECOND inbound note (different id) → MUST re-emit again. This is the line
    // that failed before the fix (domain rev was frozen at 1).
    observer.on_kernel_event(&inbound_note(
        "evt-real-2",
        "3333333333333333333333333333333333333333333333333333333333333333",
    ));
    let after_second = app_ref.run_typed_snapshot_projections();
    let soc = after_second
        .iter()
        .find(|p| p.schema_id == SCHEMA_SOCIAL)
        .expect("SECOND inbound note must ALSO re-emit podcast.social (the BLOCKER regression guard)");
    let val: serde_json::Value = serde_json::from_slice(&soc.payload).unwrap();
    let convo_count = val["nostr_conversations"].as_array().map(|a| a.len()).unwrap_or(0);
    assert_eq!(
        convo_count, 2,
        "both inbound notes (distinct roots) must appear in the re-emitted payload; got: {val}"
    );
    drop(handle);
    unsafe { drop(Box::from_raw(app)) };
}

/// ACTION-PATH RE-EMIT GUARD (FIX 1, the #423-class end-to-end seam).
///
/// Drives `SocialAction::ApprovePeer` through the REAL `handle_social_action`
/// dispatch (the same entry the FFI router uses) — NOT `state.approve_peer()`
/// directly and NOT a manual `domain_revs.social.fetch_add`. It proves the full
/// seam: action → mutate `ApprovedPeerStore` → persist → `infra.bump()` →
/// `podcast.social` sidecar RE-EMITS with the conversation's `trusted` flipped
/// to `true`. If a future edit drops the `infra.bump()` from the ApprovePeer
/// arm, the re-emit `expect` below fails.
#[test]
fn approve_peer_action_reemits_social_with_trusted_flipped_real_path() {
    use crate::ffi::actions::social_module::SocialAction;
    use crate::host_op_handler::PodcastHostOpHandler;

    let peer = "5555555555555555555555555555555555555555555555555555555555555555";

    let app = nmp_ffi::nmp_app_new();
    assert!(!app.is_null());
    let app_ref = unsafe { &*app };

    let (handle, state, _approved) = make_handle_and_state_with_approved(app);
    register_domain_projections(app_ref, &handle);

    // Seed an inbound note from a NON-followed, NON-approved peer so the
    // conversation initially projects trusted:false.
    let observer = make_social_observer(&handle);
    let _ = app_ref.run_typed_snapshot_projections(); // consume initial tombstone
    observer.on_kernel_event(&inbound_note("evt-approve-action", peer));

    let before = app_ref.run_typed_snapshot_projections();
    let soc_before = before
        .iter()
        .find(|p| p.schema_id == SCHEMA_SOCIAL)
        .expect("inbound note must emit podcast.social");
    let val_before: serde_json::Value = serde_json::from_slice(&soc_before.payload).unwrap();
    let convos_before = val_before["nostr_conversations"].as_array().cloned().unwrap_or_default();
    assert_eq!(convos_before.len(), 1, "one conversation expected; got: {val_before}");
    assert_eq!(
        convos_before[0]["trusted"], serde_json::Value::Bool(false),
        "untrusted peer's conversation must project trusted:false before approval"
    );
    // Drain so last_emitted catches up → next emit proves a real re-emit.
    assert!(
        run_domain_projections_only(app_ref).is_empty(),
        "social sidecar must idle before the approve action"
    );

    // Dispatch ApprovePeer through the REAL handler (same state Arc).
    let handler = PodcastHostOpHandler::new(app, Arc::clone(&state));
    let resp = handler.handle_social_action(
        SocialAction::ApprovePeer { pubkey_hex: peer.to_string() },
        "corr-approve-action",
    );
    assert_eq!(resp, serde_json::json!({"ok": true}), "approve action must ack ok");

    // The action's infra.bump() must drive a social re-emit carrying trusted:true.
    let after = app_ref.run_typed_snapshot_projections();
    let soc_after = after
        .iter()
        .find(|p| p.schema_id == SCHEMA_SOCIAL)
        .expect("ApprovePeer action MUST re-emit podcast.social (proves the action arm bumps domain_revs.social)");
    let val_after: serde_json::Value = serde_json::from_slice(&soc_after.payload).unwrap();
    let convos_after = val_after["nostr_conversations"].as_array().cloned().unwrap_or_default();
    assert_eq!(convos_after.len(), 1, "still one conversation after approve; got: {val_after}");
    assert_eq!(
        convos_after[0]["trusted"], serde_json::Value::Bool(true),
        "approve action must flip the conversation's trusted flag to true in the re-emitted payload"
    );

    drop(handle);
    unsafe { drop(Box::from_raw(app)) };
}

/// Companion guard for the BLOCK arm: a FOLLOWED peer's conversation projects
/// trusted:true, then `SocialAction::BlockPeer` (real dispatch) re-emits
/// podcast.social with trusted flipped to false (block is absolute, overriding
/// follow). Proves the BlockPeer arm also bumps the social domain rev.
#[test]
fn block_peer_action_reemits_social_with_trusted_false_overriding_follow() {
    use crate::ffi::actions::social_module::SocialAction;
    use crate::host_op_handler::PodcastHostOpHandler;

    let me = "ee11223344556677889900aabbccddeeff00112233445566778899aabbccddee";
    let peer = "6666666666666666666666666666666666666666666666666666666666666666";

    let app = nmp_ffi::nmp_app_new();
    assert!(!app.is_null());
    let app_ref = unsafe { &*app };

    // Build state with social wired to BOTH a follow set (containing peer) AND
    // the shared approved store, mirroring register.rs.
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let mut state_inner = PodcastAppState::new(Infra::for_test(), store.clone());
    state_inner.tasks.tasks.lock().unwrap().clear();
    let approved = Arc::new(Mutex::new(
        crate::store::approved_peer_store::ApprovedPeerStore::new(),
    ));
    let active_slot = Arc::new(Mutex::new(Some(me.to_string())));
    let follow_set = nmp_nip02::ActiveFollowSet::new(Arc::clone(&active_slot));
    follow_set.on_kernel_event(&KernelEvent {
        id: "0000000000000000000000000000000000000000000000000000000000000003".to_string(),
        author: me.to_string(),
        kind: 3,
        created_at: 200,
        tags: vec![vec!["p".to_string(), peer.to_string()]],
        content: String::new(),
        relay_provenance: vec![],
    });
    state_inner.social = crate::state::social::SocialState::new(state_inner.social.infra.clone())
        .with_follow_set(follow_set)
        .with_approved_peers(Arc::clone(&approved));
    let state = Arc::new(state_inner);
    let handle = Arc::new(PodcastHandle {
        app,
        state: Arc::clone(&state),
        responder_cache: Arc::new(Mutex::new(
            crate::store::agent_note_responder_cache::ResponderCache::default(),
        )),
        outbound_turn_cache: Arc::new(Mutex::new(
            crate::store::outbound_turn_cache::OutboundTurnCache::new(),
        )),
        approved_peer_store: Arc::clone(&approved),
        snapshot_cache: Arc::new(Mutex::new(None)),
        clean_html_cache: Arc::new(Mutex::new(HashMap::new())),
        ask_state: Arc::new(Mutex::new(crate::ffi::agent_ask::AgentAskState::default())),
        ask_callback: Arc::new(Mutex::new(crate::ffi::agent_ask::AgentAskCallbackState::default())),
    });
    register_domain_projections(app_ref, &handle);

    let observer = make_social_observer(&handle);
    let _ = app_ref.run_typed_snapshot_projections();
    observer.on_kernel_event(&inbound_note("evt-block-action", peer));

    let before = app_ref.run_typed_snapshot_projections();
    let soc_before = before
        .iter()
        .find(|p| p.schema_id == SCHEMA_SOCIAL)
        .expect("inbound note must emit podcast.social");
    let val_before: serde_json::Value = serde_json::from_slice(&soc_before.payload).unwrap();
    assert_eq!(
        val_before["nostr_conversations"][0]["trusted"], serde_json::Value::Bool(true),
        "followed peer's conversation must project trusted:true before block; got: {val_before}"
    );
    assert!(
        run_domain_projections_only(app_ref).is_empty(),
        "social sidecar must idle before the block action"
    );

    let handler = PodcastHostOpHandler::new(app, Arc::clone(&state));
    let resp = handler.handle_social_action(
        SocialAction::BlockPeer { pubkey_hex: peer.to_string() },
        "corr-block-action",
    );
    assert_eq!(resp, serde_json::json!({"ok": true}), "block action must ack ok");

    let after = app_ref.run_typed_snapshot_projections();
    let soc_after = after
        .iter()
        .find(|p| p.schema_id == SCHEMA_SOCIAL)
        .expect("BlockPeer action MUST re-emit podcast.social");
    let val_after: serde_json::Value = serde_json::from_slice(&soc_after.payload).unwrap();
    assert_eq!(
        val_after["nostr_conversations"][0]["trusted"], serde_json::Value::Bool(false),
        "block action must flip trusted to false even for a followed peer (block is absolute); got: {val_after}"
    );

    drop(handle);
    unsafe { drop(Box::from_raw(app)) };
}

// ── Slice-local payload key assertions ───────────────────────────────────────

/// Assert that the `podcast.playback` sidecar payload contains ONLY
/// `now_playing`, `queue`, and `rev` — NOT library-domain fields like
/// `library`, `settings`, `categories`, `active_account`, or `widget`.
///
/// This is the structural proof that `build_playback_payload` is slice-local:
/// if it were calling `build_podcast_update` it would produce a payload with
/// all ~30 PodcastUpdate fields, not just the three playback fields.
#[test]
fn playback_payload_contains_only_playback_keys() {
    let app = nmp_ffi::nmp_app_new();
    assert!(!app.is_null());
    let app_ref = unsafe { &*app };
    let handle = Arc::new(*make_test_handle_with_app(app));
    register_domain_projections(app_ref, &handle);

    // First run emits all domains.
    let first = app_ref.run_typed_snapshot_projections();
    let playback = first
        .iter()
        .find(|p| p.schema_id == SCHEMA_PLAYBACK)
        .expect("podcast.playback must be emitted on initial run");

    let val: serde_json::Value =
        serde_json::from_slice(&playback.payload).expect("playback payload must be valid JSON");
    let obj = val.as_object().expect("playback payload must be a JSON object");

    // Required keys.
    assert!(obj.contains_key("rev"),         "playback payload must contain 'rev'");
    assert!(obj.contains_key("now_playing"), "playback payload must contain 'now_playing'");
    assert!(obj.contains_key("queue"),       "playback payload must contain 'queue'");

    // Prohibited library-domain keys — their presence means the builder is
    // still calling build_podcast_update and fan-in is happening.
    for prohibited in &[
        "library", "categories", "settings", "active_account", "widget",
        "wiki_articles", "picks", "agent_tasks", "social",
    ] {
        assert!(
            !obj.contains_key(*prohibited),
            "playback payload must NOT contain '{prohibited}' — \
             this key only exists in build_podcast_update fan-in; \
             payload keys: {:?}",
            obj.keys().collect::<Vec<_>>()
        );
    }

    drop(handle);
    unsafe { drop(Box::from_raw(app)) };
}

/// Assert that the `podcast.settings` sidecar payload contains ONLY
/// `settings`, `configured_relays`, and `rev` — NOT library/playback fields.
#[test]
fn settings_payload_contains_only_settings_keys() {
    let app = nmp_ffi::nmp_app_new();
    assert!(!app.is_null());
    let app_ref = unsafe { &*app };
    let handle = Arc::new(*make_test_handle_with_app(app));
    register_domain_projections(app_ref, &handle);

    let first = app_ref.run_typed_snapshot_projections();
    let settings = first
        .iter()
        .find(|p| p.schema_id == SCHEMA_SETTINGS)
        .expect("podcast.settings must be emitted on initial run");

    let val: serde_json::Value =
        serde_json::from_slice(&settings.payload).expect("settings payload must be valid JSON");
    let obj = val.as_object().expect("settings payload must be a JSON object");

    assert!(obj.contains_key("rev"),               "settings payload must contain 'rev'");
    assert!(obj.contains_key("settings"),          "settings payload must contain 'settings'");
    assert!(obj.contains_key("configured_relays"), "settings payload must contain 'configured_relays'");

    for prohibited in &[
        "library", "now_playing", "queue", "downloads", "active_account",
        "widget", "wiki_articles", "picks",
    ] {
        assert!(
            !obj.contains_key(*prohibited),
            "settings payload must NOT contain '{prohibited}'; got: {:?}",
            obj.keys().collect::<Vec<_>>()
        );
    }

    drop(handle);
    unsafe { drop(Box::from_raw(app)) };
}

// ── User-curated podcast categories (D0/D4) ─────────────────────────────────

#[test]
fn set_podcast_user_categories_bumps_library_domain() {
    use crate::host_op_handler::PodcastHostOpHandler;
    use crate::state::{Infra, PodcastAppState};
    use nmp_core::substrate::HostOpHandler;
    use std::sync::atomic::Ordering;
    use std::sync::{Arc, Mutex};

    // Build a test handler (no NmpApp needed for this action).
    let store = Arc::new(Mutex::new(crate::store::PodcastStore::new()));
    let state = Arc::new(PodcastAppState::new(Infra::for_test(), store.clone()));
    state.tasks.tasks.lock().unwrap().clear();
    let handler = PodcastHostOpHandler::new(std::ptr::null_mut(), state.clone());

    let lib_rev_before = state.infra.domain_revs.library.load(Ordering::Relaxed);
    let global_rev_before = state.infra.rev.load(Ordering::Relaxed);

    // Dispatch through the namespace router using the envelope format.
    let action_json = r#"{"ns":"podcast","action":{"op":"set_podcast_user_categories","podcast_id":"11111111-1111-1111-1111-111111111111","categories":["AI","News"]}}"#;
    let result = handler.handle(action_json, "test-corr");
    assert_eq!(result["ok"], true, "action should succeed: {:?}", result);

    let lib_rev_after = state.infra.domain_revs.library.load(Ordering::Relaxed);
    let global_rev_after = state.infra.rev.load(Ordering::Relaxed);

    assert!(lib_rev_after > lib_rev_before, "library domain rev must advance");
    assert!(global_rev_after > global_rev_before, "global rev must advance");

    // Verify store was mutated.
    let store_guard = store.lock().unwrap();
    assert_eq!(
        store_guard.podcast_user_categories_for("11111111-1111-1111-1111-111111111111"),
        &["AI", "News"]
    );
}

#[test]
fn set_podcast_user_categories_noop_does_not_bump_library_domain() {
    use crate::host_op_handler::PodcastHostOpHandler;
    use crate::state::{Infra, PodcastAppState};
    use nmp_core::substrate::HostOpHandler;
    use std::sync::atomic::Ordering;
    use std::sync::{Arc, Mutex};

    let store = Arc::new(Mutex::new(crate::store::PodcastStore::new()));
    // Pre-populate so the no-op is genuine.
    store.lock().unwrap().set_podcast_user_categories(
        "22222222-2222-2222-2222-222222222222",
        vec!["AI".to_string()],
    );

    let state = Arc::new(PodcastAppState::new(Infra::for_test(), store.clone()));
    state.tasks.tasks.lock().unwrap().clear();
    let handler = PodcastHostOpHandler::new(std::ptr::null_mut(), state.clone());

    let lib_rev_before = state.infra.domain_revs.library.load(Ordering::Relaxed);

    let action_json = r#"{"ns":"podcast","action":{"op":"set_podcast_user_categories","podcast_id":"22222222-2222-2222-2222-222222222222","categories":["AI"]}}"#;
    let result = handler.handle(action_json, "test-corr");
    assert_eq!(result["ok"], true);

    let lib_rev_after = state.infra.domain_revs.library.load(Ordering::Relaxed);
    assert_eq!(lib_rev_after, lib_rev_before, "no-op must NOT bump domain rev");
}

#[test]
fn user_categories_appear_in_library_snapshot() {
    use crate::ffi::snapshot_library::build_library_snapshot;
    use podcast_core::Podcast;

    // Real (unstarted) NmpApp so build_library_snapshot's clean_html path is safe.
    let app = nmp_ffi::nmp_app_new();
    let handle = make_test_handle_with_app(app);

    // Subscribe a podcast and assign user-curated category labels to it.
    let podcast_id_str;
    {
        let mut s = handle.state.library.store.lock().unwrap();
        let mut podcast = Podcast::new("Categorized Show");
        podcast.feed_url = Some(url::Url::parse("https://example.com/feed.xml").unwrap());
        podcast_id_str = podcast.id.0.to_string();
        s.subscribe(podcast, Vec::new());
        assert!(s.set_podcast_user_categories(&podcast_id_str, vec!["AI".into(), "News".into()]));
    }

    let (transcripts, categories_cache) = (HashMap::new(), HashMap::new());
    let library = {
        let s = handle.state.library.store.lock().unwrap();
        build_library_snapshot(&handle, &s, &transcripts, &categories_cache)
    };

    let row = library
        .iter()
        .find(|p| p.id == podcast_id_str)
        .expect("subscribed podcast must project");
    assert_eq!(row.user_categories, vec!["AI".to_string(), "News".to_string()]);

    // Wire contract: present when non-empty.
    let json = serde_json::to_string(row).expect("encode");
    assert!(json.contains(r#""user_categories":["AI","News"]"#));

    drop(handle);
    unsafe { drop(Box::from_raw(app)) };
}
