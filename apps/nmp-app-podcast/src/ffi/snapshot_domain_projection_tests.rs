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
//!
//! Identity/widget tombstone and social tests are in
//! `snapshot_domain_social_tests.rs`; voice, payload-key, and user-category
//! tests are in `snapshot_domain_voice_payload_tests.rs`. Both files use the
//! `pub(super)` helpers below via `super::tests::xxx`.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use nmp_core::substrate::KernelEvent;
use nmp_core::{encode_snapshot_frame, SnapshotEnvelope, TypedProjectionData};

use crate::agent_note_handler::AgentNotesObserver;
use crate::ffi::handle::PodcastHandle;
use crate::ffi::snapshot_domain_projections::{
    decode_podcast_domain_sidecars, register_domain_projections, SCHEMA_DOWNLOADS,
    SCHEMA_LIBRARY, SCHEMA_MISC, SCHEMA_PLAYBACK, SCHEMA_SETTINGS,
};
use crate::state::{Domain, DomainRevs, Infra, PodcastAppState};
use crate::store::PodcastStore;

// ── Test helpers ──────────────────────────────────────────────────────────────

/// Make a handle with a real (unstarted) `NmpApp` so `build_configured_relays`
/// does not deref a null pointer. The caller is responsible for freeing `app`
/// after dropping the handle.
pub(super) fn make_test_handle_with_app(app: *mut nmp_ffi::NmpApp) -> Box<PodcastHandle> {
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
pub(super) fn make_handle_and_state_with_approved(
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

pub(super) fn make_frame_with_sidecars(sidecars: &[TypedProjectionData]) -> Vec<u8> {
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
pub(super) fn run_domain_projections_only(app_ref: &nmp_ffi::NmpApp) -> Vec<TypedProjectionData> {
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
pub(super) fn make_social_observer(handle: &Arc<PodcastHandle>) -> AgentNotesObserver {
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
pub(super) fn inbound_note(id: &str, author_hex: &str) -> KernelEvent {
    KernelEvent {
        id: id.to_string(),
        author: author_hex.to_string(),
        kind: 1,
        created_at: 1_717_200_000,
        tags: Vec::new(),
        content: "hello from a followed peer".to_string(),
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
