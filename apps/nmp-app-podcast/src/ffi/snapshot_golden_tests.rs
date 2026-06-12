//! Golden byte-identity test for `build_podcast_update`.
//!
//! ## Purpose
//!
//! Every step of the god-root state-consolidation refactor (design doc at
//! `docs/design/podcast-app-state-refactor.md`) must leave the snapshot wire
//! format **byte-identical** to the captured fixture.  Any diff means the step
//! accidentally changed the projection, not just the state topology.
//!
//! ## Fixture lifecycle
//!
//! The fixture at `src/ffi/snapshot_golden_fixture.json` is **generated** from
//! this test on first run (when it does not exist).  After that every run
//! asserts byte-identical match.  If you intentionally change the projection
//! format you must:
//!
//!  1. Delete `src/ffi/snapshot_golden_fixture.json`.
//!  2. Re-run the test suite once to regenerate it.
//!  3. Commit the new fixture in the same PR that changed the format.
//!
//! ## Determinism contract
//!
//! All IDs and timestamps are **fixed** so the output is byte-identical across
//! runs.  The fixture is built from a minimal, representative `PodcastHandle`
//! with a null `app` pointer — the projection path under test never dereferences
//! `app`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use chrono::{TimeZone, Utc};
use podcast_core::{Episode, Podcast, PodcastId};
use url::Url;
use uuid::Uuid;

use crate::ffi::handle::PodcastHandle;
use crate::ffi::snapshot::build_podcast_update;
use crate::store::identity::IdentityStore;
use crate::store::PodcastStore;

// ── Fixed-ID constants ────────────────────────────────────────────────────────

/// Deterministic podcast UUID — must never change.
const PODCAST_UUID: &str = "a1a1a1a1-b2b2-c3c3-d4d4-e5e5e5e5e5e5";
/// Fixed UNIX epoch for all timestamps (2024-01-01T00:00:00Z).
const FIXED_EPOCH: i64 = 1_704_067_200;

// ── Fixture path ──────────────────────────────────────────────────────────────

fn fixture_path() -> PathBuf {
    // CARGO_MANIFEST_DIR is set by `cargo test` to the package root.
    let manifest = std::env::var("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR set by cargo test");
    PathBuf::from(manifest).join("src/ffi/snapshot_golden_fixture.json")
}

// ── Handle construction ───────────────────────────────────────────────────────

fn make_golden_handle(app: *mut nmp_ffi::NmpApp) -> Box<PodcastHandle> {
    let store = {
        let mut s = PodcastStore::new();

        let pid = PodcastId::new(Uuid::parse_str(PODCAST_UUID).unwrap());
        let podcast = Podcast {
            id: pid,
            feed_url: Some(Url::parse("https://example.com/feed.xml").unwrap()),
            title: "Golden Test Show".to_owned(),
            author: "Golden Author".to_owned(),
            image_url: None,
            description: "A fixed, deterministic podcast.".to_owned(),
            language: None,
            categories: vec!["Technology".to_owned()],
            discovered_at: Utc.timestamp_opt(FIXED_EPOCH, 0).unwrap(),
            owner_pubkey_hex: None,
            nostr_visibility: podcast_core::NostrVisibility::Private,
            nostr_coordinate: None,
            title_is_placeholder: false,
            last_refreshed_at: None,
            etag: None,
            last_modified: None,
        };

        // Deterministic EpisodeId: `EpisodeId::from_feed_and_guid` is stable.
        let ep = Episode::new(
            pid,
            "https://example.com/feed.xml",
            "golden-guid-001",
            "Golden Episode One",
            Url::parse("https://example.com/audio/ep1.mp3").unwrap(),
            Utc.timestamp_opt(FIXED_EPOCH, 0).unwrap(),
        );

        s.subscribe(podcast, vec![ep]);
        Arc::new(Mutex::new(s))
    };

    let rev = Arc::new(AtomicU64::new(1));
    let identity = Arc::new(Mutex::new(IdentityStore::new()));
    let state = Arc::new(crate::state::PodcastAppState::new_with_identity(
        crate::state::Infra::for_test(),
        store.clone(),
        identity.clone(),
    ));
    // Clear agent_tasks: default_seed() uses Uuid::new_v4() + Utc::now(), making
    // the fixture non-deterministic.  The golden test exercises the projection
    // *shape*, not the task content — leave the slot empty so the fixture is
    // byte-identical across runs (skip_serializing_if = "Vec::is_empty" omits it).
    state.tasks.tasks.lock().unwrap().clear();

    // Steps 8-10: search_results, nostr_results, comments_cache,
    // viewed_comments_episode_id, social, agent_notes removed — now owned by
    // state.discovery / state.comments / state.social respectively.
    // Step 14: player_actor, queue, download_queue removed — now owned by
    // state.playback (PlaybackState).
    Box::new(PodcastHandle {
        app,
        state,
        store: store.clone(),
        identity,
        rev: rev.clone(),
        snapshot_signal: None,
        snapshot_cache: Arc::new(Mutex::new(None)),
        clean_html_cache: Arc::new(Mutex::new(HashMap::new())),
        // player_actor removed in Step 14 — now owned by state.playback.player.
        // queue removed in Step 14 — now owned by state.playback.queue.
        // download_queue removed in Step 14 — now owned by state.playback.downloads.
        // clips, transcripts, agent_tasks removed in Steps 5a, 5b, 6 —
        // now owned by state.clips / state.transcripts / state.tasks.
        // dismissed_episode_ids, inbox_triage_cache, inbox_triage_in_progress removed in Step 7 —
        // now owned by state.inbox (InboxState).
        // podcast_keys and publish_state removed in Step 13 —
        // now owned by state.publish (PublishState).
        // voice_state and voice_conversation removed in Step 12 —
        // now owned by state.voice (VoiceSubstate).
        // conversation, agent_busy, agent_touched removed in Step 11 —
        // now owned by state.agent_chat (AgentChatState).
        feedback: nmp_feedback::FeedbackRuntime::new(
            nmp_feedback::FeedbackConfig::new(crate::PODCAST_FEEDBACK_PROJECT_COORDINATE)
                .with_interest_namespace(crate::PODCAST_FEEDBACK_INTEREST_NAMESPACE),
            Arc::new(Mutex::new(Vec::new())),
            rev.clone(),
        ),
        runtime: Arc::new(tokio::runtime::Runtime::new().unwrap()),
        feed_fetch: crate::feed_fetch::FeedFetchCoordinator::new_test(),
    })
}

// ── Golden test ───────────────────────────────────────────────────────────────

#[test]
fn snapshot_bytes_match_golden_fixture() {
    // A real (unstarted) NmpApp so the configured-relays projection doesn't
    // deref a null pointer.  Never started — no background thread touches it.
    let app = nmp_ffi::nmp_app_new();
    assert!(!app.is_null(), "nmp_app_new returned null");

    let handle = make_golden_handle(app);
    let update = build_podcast_update(&handle);
    let actual_json = serde_json::to_string(&update).expect("serialize PodcastUpdate");

    // Free the app AFTER the snapshot is built and handle is dropped.
    // SAFETY: `app` came from `nmp_app_new` and is freed exactly once here.
    // It was never started, so there is no actor thread to join.
    drop(handle);
    unsafe { drop(Box::from_raw(app)) };

    let path = fixture_path();

    if !path.exists() {
        // First run: generate the fixture. Commit the result.
        std::fs::write(&path, &actual_json).expect("write golden fixture");
        println!(
            "Golden fixture written ({} bytes).\n\
             Path: {}\n\
             Commit this file — subsequent runs assert byte-identical match.",
            actual_json.len(),
            path.display()
        );
        // First run is always green (capture run).
        return;
    }

    let expected = std::fs::read_to_string(&path).expect("read golden fixture");

    assert_eq!(
        actual_json, expected,
        "Snapshot wire format changed — golden test FAILED.\n\
         This means the refactor altered the projection output. Either:\n\
         1. Intentional format change → delete the fixture and re-run to recapture.\n\
         2. Accidental regression → revert the step that caused the diff.\n\n\
         Expected {} bytes, got {} bytes.",
        expected.len(),
        actual_json.len(),
    );
}
