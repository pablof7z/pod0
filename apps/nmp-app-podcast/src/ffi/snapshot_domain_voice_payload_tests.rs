//! Voice domain projection, slice-local payload key, and user-category tests.
//!
//! Split from `snapshot_domain_projection_tests.rs` to comply with the 500-line
//! hard limit (AGENTS.md). The shared `make_test_handle_with_app` and
//! `run_domain_projections_only` helpers are `pub(super)` in the `tests` module
//! (see `snapshot_domain_projection_tests.rs`) and accessed here via
//! `super::tests::xxx`.

use std::collections::HashMap;
use std::sync::Arc;

use crate::ffi::snapshot_domain_projections::{
    register_domain_projections, SCHEMA_PLAYBACK, SCHEMA_SETTINGS, SCHEMA_VOICE,
};

use super::tests::{make_test_handle_with_app, run_domain_projections_only};

// ── Voice domain tombstone + delta isolation ──────────────────────────────────

/// `podcast.voice` empty/idle → tombstone on first run, then idles on second tick.
///
/// Voice state defaults to idle (all false). When emitted, the closure
/// detects the idle state via `build_voice_payload` returning `None` and
/// emits a `voice_tombstone` so iOS/Android decoders know voice is idle.
/// A second tick with the same idle state returns `None` (no perpetual rebuild).
#[test]
fn voice_idle_emits_tombstone_then_idles() {
    let app = Box::into_raw(Box::new(nmp_native_runtime::new_app()));
    assert!(!app.is_null());
    let app_ref = unsafe { &*app };
    let handle = Arc::new(*make_test_handle_with_app(app));
    register_domain_projections(app_ref, &handle);

    // First run: rev 1 > last_emitted 0; voice is idle → tombstone.
    let first = app_ref.run_typed_snapshot_projections();
    let voice = first
        .iter()
        .find(|p| p.schema_id == SCHEMA_VOICE)
        .expect("voice tombstone must be emitted when state is idle");
    let val: serde_json::Value = serde_json::from_slice(&voice.payload).unwrap();
    assert_eq!(
        val["voice"],
        serde_json::Value::Null,
        "tombstone must carry voice: null"
    );
    assert!(val["rev"].is_number(), "tombstone must carry a rev field");

    // Second tick — last_emitted caught up → no voice sidecar (no perpetual rebuild).
    let second = app_ref.run_typed_snapshot_projections();
    assert!(
        second.iter().all(|p| p.schema_id != SCHEMA_VOICE),
        "second idle tick must NOT emit voice sidecar"
    );

    drop(handle);
    unsafe { drop(Box::from_raw(app)) };
}

/// Delta isolation: voice report via the real mutation path emits ONLY
/// `podcast.voice` (not library, playback, settings, etc.).
///
/// This test drives a real voice report mutation through `voice_handler::mutate_voice_state`
/// (the production path that bumps `domain_revs.voice`) and asserts that ONLY
/// the voice sidecar is emitted, proving the mutation route is domain-scoped
/// and no broader sidecars are affected.
#[test]
fn voice_report_emits_only_voice_sidecar() {
    use crate::host_op_handler::PodcastHostOpHandler;

    let app = Box::into_raw(Box::new(nmp_native_runtime::new_app()));
    assert!(!app.is_null());
    let app_ref = unsafe { &*app };

    let handle = Arc::new(*make_test_handle_with_app(app));
    register_domain_projections(app_ref, &handle);

    // Consume initial state.
    let _ = app_ref.run_typed_snapshot_projections();
    let no_change = run_domain_projections_only(app_ref);
    assert!(
        no_change.is_empty(),
        "second run with no bump must emit nothing; got {:?}",
        no_change
            .iter()
            .map(|p| p.schema_id.as_str())
            .collect::<Vec<_>>()
    );

    // Create a handler and mutate voice state directly (simulates voice report arrival).
    let handler = PodcastHostOpHandler::new(app, Arc::clone(&handle.state));
    crate::voice_handler::mutate_voice_state(&handler, |v| {
        v.is_listening = true;
    });

    let after = app_ref.run_typed_snapshot_projections();
    let keys: Vec<&str> = after.iter().map(|p| p.schema_id.as_str()).collect();

    assert!(
        keys.contains(&SCHEMA_VOICE),
        "podcast.voice must be emitted after voice state mutation; got {keys:?}"
    );
    assert!(
        !keys.contains(&"podcast.library"),
        "podcast.library must NOT be emitted after a voice-only mutation (delta isolation); got {keys:?}"
    );
    assert!(
        !keys.contains(&"podcast.playback"),
        "podcast.playback must NOT be emitted after a voice-only mutation; got {keys:?}"
    );
    assert!(
        !keys.contains(&"podcast.settings"),
        "podcast.settings must NOT be emitted after a voice-only mutation; got {keys:?}"
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
    let app = Box::into_raw(Box::new(nmp_native_runtime::new_app()));
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
    let obj = val
        .as_object()
        .expect("playback payload must be a JSON object");

    // Required keys.
    assert!(
        obj.contains_key("rev"),
        "playback payload must contain 'rev'"
    );
    assert!(
        obj.contains_key("now_playing"),
        "playback payload must contain 'now_playing'"
    );
    assert!(
        obj.contains_key("queue"),
        "playback payload must contain 'queue'"
    );

    // Prohibited library-domain keys — their presence means the builder is
    // still calling build_podcast_update and fan-in is happening.
    for prohibited in &[
        "library",
        "categories",
        "settings",
        "active_account",
        "widget",
        "wiki_articles",
        "picks",
        "agent_tasks",
        "social",
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
    let app = Box::into_raw(Box::new(nmp_native_runtime::new_app()));
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
    let obj = val
        .as_object()
        .expect("settings payload must be a JSON object");

    assert!(
        obj.contains_key("rev"),
        "settings payload must contain 'rev'"
    );
    assert!(
        obj.contains_key("settings"),
        "settings payload must contain 'settings'"
    );
    assert!(
        obj.contains_key("configured_relays"),
        "settings payload must contain 'configured_relays'"
    );

    for prohibited in &[
        "library",
        "now_playing",
        "queue",
        "downloads",
        "active_account",
        "widget",
        "wiki_articles",
        "picks",
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

    assert!(
        lib_rev_after > lib_rev_before,
        "library domain rev must advance"
    );
    assert!(
        global_rev_after > global_rev_before,
        "global rev must advance"
    );

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
    assert_eq!(
        lib_rev_after, lib_rev_before,
        "no-op must NOT bump domain rev"
    );
}

#[test]
fn user_categories_appear_in_library_snapshot() {
    use crate::ffi::snapshot_library::build_library_snapshot;
    use podcast_core::Podcast;

    // Real (unstarted) NmpApp so build_library_snapshot's clean_html path is safe.
    let app = Box::into_raw(Box::new(nmp_native_runtime::new_app()));
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
    assert_eq!(
        row.user_categories,
        vec!["AI".to_string(), "News".to_string()]
    );

    // Wire contract: present when non-empty.
    let json = serde_json::to_string(row).expect("encode");
    assert!(json.contains(r#""user_categories":["AI","News"]"#));

    drop(handle);
    unsafe { drop(Box::from_raw(app)) };
}
