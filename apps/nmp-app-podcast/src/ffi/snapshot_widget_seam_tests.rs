//! End-to-end seam test for the kernel-owned widget projection.
//!
//! The unit tests in `snapshot_widget_tests.rs` prove `build_widget_snapshot`
//! is correct *given a populated `PlayerState`*. This file pins the layer
//! above it: that driving the **real** entry points the iOS shell uses —
//! the `podcast.player` `play` host-op (which stages the actor) followed by an
//! `AudioReport::Playing` through the real `nmp_app_podcast_audio_report` FFI —
//! leaves BOTH `PodcastUpdate.now_playing` AND `PodcastUpdate.widget` populated
//! and mutually consistent.
//!
//! This is the regression pin for the live-simulator bug caught while
//! verifying #366/#371: the App Group widget JSON always carried the idle
//! shape during real playback. The decisive question was whether the kernel
//! emits an idle `WidgetSnapshot` when driven correctly — it does not, as long
//! as the play path routes through the host-op that calls `stage_load` (which
//! is the only place the actor's `episode_id` is set; `on_playing` alone never
//! sets it). If this test is green but the live widget is still idle, the gap
//! is in what the shell dispatches, not the kernel derivation.

use std::collections::HashMap;
use std::ffi::CString;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use podcast_core::{Episode, Podcast};
use url::Url;

use crate::ffi::audio_report::nmp_app_podcast_audio_report;
use crate::ffi::handle::PodcastHandle;
use crate::ffi::snapshot::build_podcast_update;
use crate::host_op_handler::PodcastHostOpHandler;
use crate::store::identity::IdentityStore;
use crate::store::PodcastStore;
use nmp_core::substrate::HostOpHandler;
// DownloadQueue and PlaybackQueue removed in Step 14 — now in PlaybackState.

/// Shared kernel state — the exact Arcs `nmp_app_podcast_register` clones into
/// both the host-op handler (writer) and the handle (snapshot reader).
///
/// Step 14: `player_actor` removed — it now lives inside
/// `PodcastAppState.playback.player`.  Both seams share the SAME
/// `Arc<PodcastAppState>` so writes the handler makes are visible when the
/// handle projects the snapshot.
struct SharedKernel {
    store: Arc<Mutex<PodcastStore>>,
    state: Arc<crate::state::PodcastAppState>,
    rev: Arc<AtomicU64>,
}

fn feedback_runtime(rev: Arc<AtomicU64>) -> nmp_feedback::FeedbackRuntime {
    nmp_feedback::FeedbackRuntime::new(
        nmp_feedback::FeedbackConfig::new(crate::PODCAST_FEEDBACK_PROJECT_COORDINATE)
            .with_interest_namespace(crate::PODCAST_FEEDBACK_INTEREST_NAMESPACE),
        Arc::new(Mutex::new(Vec::new())),
        rev,
    )
}

/// Build a `PodcastHostOpHandler` that shares the caller's `state` (which
/// owns `player_actor` in `state.playback.player`) so writes are visible to a
/// handle built from the same `Arc<PodcastAppState>`.
///
/// Step 14: `player_actor`, `queue`, and `download_queue` are no longer
/// separate constructor args — they live inside `state.playback`.
fn handler_sharing(shared: &SharedKernel, app: *mut nmp_ffi::NmpApp) -> PodcastHostOpHandler {
    let identity = Arc::new(Mutex::new(IdentityStore::new()));
    PodcastHostOpHandler::new(
        app,
        shared.state.clone(),
        shared.store.clone(),
        identity,
        shared.rev.clone(),
        Arc::new(tokio::runtime::Runtime::new().unwrap()),
        crate::feed_fetch::FeedFetchCoordinator::new_test(),
        feedback_runtime(shared.rev.clone()),
    )
}

/// Build a `PodcastHandle` sharing the SAME `Arc<PodcastAppState>` as the
/// handler so the snapshot reader sees the writer's mutations without any
/// separate Arc wiring.
fn handle_sharing(shared: &SharedKernel, app: *mut nmp_ffi::NmpApp) -> Box<PodcastHandle> {
    let identity = Arc::new(Mutex::new(IdentityStore::new()));
    Box::new(PodcastHandle {
        app,
        state: shared.state.clone(),
        // player_actor removed in Step 14 — now state.playback.player.
        store: shared.store.clone(),
        identity,
        rev: shared.rev.clone(),
        snapshot_signal: None,
        snapshot_cache: Arc::new(Mutex::new(None)),
        clean_html_cache: Arc::new(Mutex::new(HashMap::new())),
        // queue removed in Step 14 — now state.playback.queue.
        // download_queue removed in Step 14 — now state.playback.downloads.
        feedback: feedback_runtime(shared.rev.clone()),
        runtime: Arc::new(tokio::runtime::Runtime::new().unwrap()),
        feed_fetch: crate::feed_fetch::FeedFetchCoordinator::new_test(),
    })
}

/// Subscribe one show with one streamable episode and return the episode id
/// (the canonical `Episode.id.0` UUID string the host-op + reports key on).
fn seed_one_episode(store: &Arc<Mutex<PodcastStore>>, show: &str, ep_title: &str) -> String {
    let podcast = Podcast::new(show);
    let pid = podcast.id;
    let ep = Episode::new(
        pid,
        "https://example.com/feed.xml",
        format!("guid-{}", uuid::Uuid::new_v4()),
        ep_title,
        Url::parse("https://example.com/audio.mp3").unwrap(),
        chrono::Utc::now(),
    );
    let ep_id = ep.id.0.to_string();
    store.lock().unwrap().subscribe(podcast, vec![ep]);
    ep_id
}

/// THE DECISIVE EXPERIMENT.
///
/// Drive the real iOS-shell entry points in order:
///   1. `podcast.player` `play` host-op → `handle_play` → `stage_load` (sets
///      the actor's `episode_id`).
///   2. `AudioReport::Playing` through `nmp_app_podcast_audio_report` (sets
///      `is_playing = true`, position, duration on the SAME actor).
/// Then build the snapshot via the real `build_podcast_update` and assert BOTH
/// `now_playing` and `widget` are populated and consistent.
#[test]
fn play_then_playing_report_populates_now_playing_and_widget() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let rev = Arc::new(AtomicU64::new(1));
    let identity = Arc::new(Mutex::new(IdentityStore::new()));
    // Step 14: both seams share ONE Arc<PodcastAppState> so handler writes
    // to state.playback.player are visible to the handle's snapshot reader.
    let state = Arc::new(crate::state::PodcastAppState::new_with_identity(
        crate::state::Infra::for_test(),
        store.clone(),
        identity,
    ));
    let shared = SharedKernel {
        store: store.clone(),
        state,
        rev: rev.clone(),
    };
    let ep_id = seed_one_episode(&shared.store, "The Daily", "Friday, June 6");

    // A real (unstarted) NmpApp so the snapshot's configured-relays projection
    // has a live pointer to read; we never start the actor thread — the play
    // host-op and the audio report are driven synchronously below.
    let app = nmp_ffi::nmp_app_new();
    assert!(!app.is_null(), "nmp_app_new returned null");

    let handler = handler_sharing(&shared, app);
    let handle = handle_sharing(&shared, app);

    // --- Step 1: real `play` host-op (the exact JSON the shell dispatches) ---
    // CRITICAL: the iOS shell sends `UUID.uuidString`, which Foundation renders
    // UPPERCASE, while the kernel stores the lowercase `Uuid::to_string` form.
    // Drive the host-op with the UPPERCASE id so this test is a genuine
    // regression pin for the case-mismatch root cause — before the fix,
    // `episode_playback_info`'s `==` missed and `handle_play` bailed before
    // `stage_load`, leaving now_playing + widget idle.
    let ios_episode_id = ep_id.to_uppercase();
    assert_ne!(
        ios_episode_id, ep_id,
        "the stored id must be lowercase so the uppercase iOS form exercises the case path"
    );
    let play_json = format!(
        r#"{{"ns":"podcast.player","action":{{"op":"play","episode_id":"{ios_episode_id}"}}}}"#
    );
    let resp = handler.handle(&play_json, "corr-seam-play");
    assert_eq!(
        resp["ok"],
        serde_json::json!(true),
        "play host-op must succeed for a subscribed, resolvable episode even when \
         the dispatched id is the UPPERCASE iOS `UUID.uuidString` form"
    );

    // After `play` the actor is staged: episode_id set, is_playing still false
    // (the engine hasn't reported a Playing tick yet).
    let after_play = build_podcast_update(&handle);
    let np = after_play
        .now_playing
        .as_ref()
        .expect("now_playing must be populated immediately after the play host-op stages the actor");
    assert_eq!(np.episode_id.as_deref(), Some(ep_id.as_str()));
    let widget = after_play
        .widget
        .as_ref()
        .expect("widget must be populated after stage_load — not the idle shape");
    assert_eq!(
        widget.now_playing_episode_title.as_deref(),
        Some("Friday, June 6"),
        "widget must carry the resolved episode title, not a null now_playing_episode_title"
    );

    // --- Step 2: real `AudioReport::Playing` through the FFI report path ------
    let report_json = CString::new(
        r#"{"type":"playing","url":"https://example.com/audio.mp3","position_secs":30.0,"duration_secs":1800.0}"#,
    )
    .unwrap();
    let handle_ptr = Box::into_raw(handle);
    let ret = nmp_app_podcast_audio_report(handle_ptr, report_json.as_ptr());
    // The Playing response is a `CString::into_raw` pointer; reclaim it the
    // same way (not via `nmp_app_free_string`, which is for nmp_ffi malloc
    // strings).
    if !ret.is_null() {
        let _ = unsafe { CString::from_raw(ret) };
    }
    // SAFETY: we boxed it ourselves above.
    let handle = unsafe { Box::from_raw(handle_ptr) };

    // --- Assert: snapshot now_playing AND widget are both live + consistent ---
    let update = build_podcast_update(&handle);

    let np = update
        .now_playing
        .as_ref()
        .expect("now_playing must be populated during live playback");
    assert_eq!(np.episode_id.as_deref(), Some(ep_id.as_str()));
    assert!(
        np.is_playing,
        "now_playing.is_playing must be true after a Playing report"
    );
    assert_eq!(np.position_secs, 30.0);

    let widget = update
        .widget
        .as_ref()
        .expect("widget MUST be populated during playback — the live bug was an idle widget here");
    assert!(
        widget.is_playing,
        "widget.is_playing must mirror the playing actor (live bug: always false)"
    );
    assert_eq!(
        widget.now_playing_episode_title.as_deref(),
        Some("Friday, June 6"),
        "widget MUST carry now_playing_episode_title during playback (live bug: key absent)"
    );
    assert_eq!(widget.position_secs, 30.0);
    assert_eq!(widget.duration_secs, 1800.0);
    assert!(
        (widget.position_fraction - (30.0 / 1800.0) as f32).abs() < 1e-6,
        "widget fraction must reflect the live playhead"
    );

    // Consistency: the widget and now_playing derive from the same actor state.
    assert_eq!(widget.is_playing, np.is_playing);
    assert_eq!(widget.position_secs, np.position_secs);

    // Drop our shared references before freeing the app, then reclaim it.
    drop(handler);
    drop(handle);
    // SAFETY: `app` came from `nmp_app_new` and is freed exactly once here.
    // It was never started, so there is no actor thread to join.
    unsafe {
        drop(Box::from_raw(app));
    }
}
