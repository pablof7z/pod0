//! Identity, widget, and social domain projection tests. Split from
//! `snapshot_domain_projection_tests.rs` (AGENTS.md 500-line hard limit).
//! Shared helpers live in the `tests` module and are accessed via `super::tests::xxx`.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use nmp_core::substrate::KernelEvent;
use nmp_core::KernelEventObserver;

use crate::ffi::handle::PodcastHandle;
use crate::ffi::snapshot_domain_projections::{
    register_domain_projections, SCHEMA_IDENTITY, SCHEMA_SOCIAL, SCHEMA_WIDGET,
};
use crate::state::{Infra, PodcastAppState};
use crate::store::PodcastStore;

use super::tests::{
    inbound_note, make_handle_and_state_with_approved, make_social_observer,
    make_test_handle_with_app, run_domain_projections_only,
};
// ── Tombstone contract (identity, widget) ─────────────────────────────────────

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

// ── Social domain tests ───────────────────────────────────────────────────────

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
        !keys.contains(&"podcast.library"),
        "podcast.library must NOT be emitted after a social-only mutation (delta isolation); got {keys:?}"
    );
    assert!(
        !keys.contains(&"podcast.playback"),
        "podcast.playback must NOT be emitted after a social-only mutation; got {keys:?}"
    );
    assert!(
        !keys.contains(&"podcast.misc"),
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
