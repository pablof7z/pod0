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

use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use podcast_core::{Episode, Podcast};
use url::Url;

use crate::agent_handler::AgentChatHandler;
use crate::download::DownloadQueue;
use crate::ffi::audio_report::nmp_app_podcast_audio_report;
use crate::ffi::handle::PodcastHandle;
use crate::ffi::projections::VoiceState;
use crate::ffi::snapshot::build_podcast_update;
use crate::host_op_handler::PodcastHostOpHandler;
use crate::player::PlayerActor;
use crate::queue::PlaybackQueue;
use crate::store::identity::IdentityStore;
use crate::store::{PodcastKeyStore, PodcastStore};
use nmp_core::substrate::HostOpHandler;

/// Shared kernel state — the exact Arcs `nmp_app_podcast_register` clones into
/// both the host-op handler (writer) and the handle (snapshot reader).
struct SharedKernel {
    store: Arc<Mutex<PodcastStore>>,
    player_actor: Arc<Mutex<PlayerActor>>,
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

/// Build a `PodcastHostOpHandler` that shares the caller's `store`,
/// `player_actor`, and `rev` so writes it makes are visible to a handle built
/// from the same Arcs. `app` is a real, fresh `NmpApp` (never started, so no
/// actor thread): `build_podcast_update` reads its `configured_relays_handle`
/// and `dispatch_audio` derefs it (a no-op send into an unstarted app).
fn handler_sharing(shared: &SharedKernel, app: *mut nmp_ffi::NmpApp) -> PodcastHostOpHandler {
    let agent_chat = AgentChatHandler::new_without_runtime(
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
        shared.rev.clone(),
    );
    let state = Arc::new(crate::state::PodcastAppState::new(
        crate::state::Infra::for_test(),
        shared.store.clone(),
    ));
    PodcastHostOpHandler::new(
        app,
        state,
        shared.store.clone(),
        Arc::new(Mutex::new(IdentityStore::new())),
        shared.player_actor.clone(),
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(Mutex::new(PlaybackQueue::new())),
        Arc::new(Mutex::new(DownloadQueue::new())),
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(Mutex::new(HashMap::new())),
        Arc::new(Mutex::new(HashSet::new())),
        Arc::new(Mutex::new(VoiceState::default())),
        Arc::new(Mutex::new(HashMap::new())),
        shared.rev.clone(),
        Arc::new(Mutex::new(PodcastKeyStore::new())),
        Arc::new(Mutex::new(HashMap::new())),
        agent_chat,
        Arc::new(Mutex::new(HashMap::new())),
        Arc::new(Mutex::new(None::<String>)),
        Arc::new(tokio::runtime::Runtime::new().unwrap()),
        Arc::new(Mutex::new(HashMap::new())),
        Arc::new(AtomicBool::new(false)),
        Arc::new(Mutex::new(None)),
        Arc::new(Mutex::new(Vec::new())),
        crate::feed_fetch::FeedFetchCoordinator::new_test(),
        feedback_runtime(shared.rev.clone()),
    )
}

/// Build a `PodcastHandle` sharing the caller's `store`, `player_actor`, and
/// `rev`, pointing at the same real `app` as the handler so
/// `build_podcast_update`'s configured-relays projection has a live pointer.
fn handle_sharing(shared: &SharedKernel, app: *mut nmp_ffi::NmpApp) -> Box<PodcastHandle> {
    let state = Arc::new(crate::state::PodcastAppState::new(
        crate::state::Infra::for_test(),
        shared.store.clone(),
    ));
    Box::new(PodcastHandle {
        app,
        state,
        player_actor: shared.player_actor.clone(),
        store: shared.store.clone(),
        identity: Arc::new(Mutex::new(IdentityStore::new())),
        rev: shared.rev.clone(),
        snapshot_signal: None,
        search_results: Arc::new(Mutex::new(Vec::new())),
        nostr_results: Arc::new(Mutex::new(Vec::new())),
        snapshot_cache: Arc::new(Mutex::new(None)),
        clean_html_cache: Arc::new(Mutex::new(HashMap::new())),
        queue: Arc::new(Mutex::new(PlaybackQueue::new())),
        download_queue: Arc::new(Mutex::new(DownloadQueue::new())),
        wiki_articles: Arc::new(Mutex::new(Vec::new())),
        wiki_search_results: Arc::new(Mutex::new(Vec::new())),
        picks: Arc::new(Mutex::new(Vec::new())),
        agent_tasks: Arc::new(Mutex::new(Vec::new())),
        clips: Arc::new(Mutex::new(Vec::new())),
        transcripts: Arc::new(Mutex::new(HashMap::new())),
        dismissed_episode_ids: Arc::new(Mutex::new(HashSet::new())),
        podcast_keys: Arc::new(Mutex::new(PodcastKeyStore::new())),
        publish_state: Arc::new(Mutex::new(HashMap::new())),
        voice_state: Arc::new(Mutex::new(VoiceState::default())),
        voice_conversation: crate::voice_conversation::VoiceConversationManager::new(
            std::ptr::null_mut(),
            Arc::new(Mutex::new(Vec::new())),
            shared.store.clone(),
            Arc::new(Mutex::new(VoiceState::default())),
            Arc::new(tokio::runtime::Runtime::new().unwrap()),
            shared.rev.clone(),
            None,
        ),
        conversation: Arc::new(Mutex::new(Vec::new())),
        agent_busy: Arc::new(AtomicBool::new(false)),
        agent_touched: Arc::new(AtomicBool::new(false)),
        categories: Arc::new(Mutex::new(HashMap::new())),
        inbox_triage_cache: Arc::new(Mutex::new(HashMap::new())),
        inbox_triage_in_progress: Arc::new(AtomicBool::new(false)),
        comments_cache: Arc::new(Mutex::new(HashMap::new())),
        viewed_comments_episode_id: Arc::new(Mutex::new(None)),
        social: Arc::new(Mutex::new(None)),
        agent_notes: Arc::new(Mutex::new(Vec::new())),
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
    let shared = SharedKernel {
        store: Arc::new(Mutex::new(PodcastStore::new())),
        player_actor: Arc::new(Mutex::new(PlayerActor::new())),
        rev: Arc::new(AtomicU64::new(1)),
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
