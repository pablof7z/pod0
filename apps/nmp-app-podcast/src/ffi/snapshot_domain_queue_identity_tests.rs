//! Byte-identity regression guard for the slice-local playback queue rows.
//!
//! Split into its own file (rather than appended to
//! `snapshot_domain_projection_tests.rs`) to keep both test files under the
//! 500-line hard limit (AGENTS.md).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::{TimeZone, Utc};
use podcast_core::{Chapter, Episode, Podcast, PodcastId};
use url::Url;
use uuid::Uuid;

use crate::ffi::handle::PodcastHandle;
use crate::ffi::snapshot::build_podcast_update;
use crate::ffi::snapshot_domain_builders::build_playback_payload;
use crate::state::{Infra, PodcastAppState};
use crate::store::PodcastStore;

/// Minimal handle with a real (unstarted) `NmpApp` so `build_configured_relays`
/// does not deref a null pointer. The caller frees `app` after dropping the
/// handle.
fn make_test_handle_with_app(app: *mut nmp_native_runtime::NmpApp) -> Box<PodcastHandle> {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let state = Arc::new(PodcastAppState::new(Infra::for_test(), store.clone()));
    state.tasks.tasks.lock().unwrap().clear();

    Box::new(PodcastHandle {
        app,
        state,
        responder_cache: Arc::new(Mutex::new(
            crate::store::agent_note_responder_cache::ResponderCache::default(),
        )),
        outbound_turn_cache: Arc::new(Mutex::new(
            crate::store::outbound_turn_cache::OutboundTurnCache::new(),
        )),
        approved_peer_store: Arc::new(Mutex::new(
            crate::store::approved_peer_store::ApprovedPeerStore::new(),
        )),
        snapshot_cache: Arc::new(Mutex::new(None)),
        clean_html_cache: Arc::new(Mutex::new(HashMap::new())),
        ask_state: Arc::new(Mutex::new(crate::ffi::agent_ask::AgentAskState::default())),
        ask_callback: Arc::new(Mutex::new(
            crate::ffi::agent_ask::AgentAskCallbackState::default(),
        )),
    })
}

/// REGRESSION GUARD (the gap the empty-queue golden fixture cannot cover):
/// a queued episode carrying real per-episode derived content (HTML
/// description, chapters, transcript) must produce a queue row in the
/// slice-local `build_playback_payload` that is BYTE-IDENTICAL to the row the
/// full `build_podcast_update` path emits for the same store/queue state.
///
/// The original slice-local builder hardcoded `description: None`, `chapters:
/// Vec::new()`, etc., so for any episode with a non-empty description the
/// `EpisodeSummary` `skip_serializing_if` fields were OMITTED — silently
/// diverging from the library path. The empty-queue golden fixture missed this
/// (queue len 0) and only ever called `build_podcast_update`, never the
/// slice-local builder. This test enqueues a content-rich episode and asserts
/// the two queue rows serialize identically.
#[test]
fn queue_row_byte_identical_to_full_snapshot_for_content_rich_episode() {
    let app = Box::into_raw(Box::new(nmp_native_runtime::new_app()));
    assert!(!app.is_null());
    let handle = Arc::new(*make_test_handle_with_app(app));

    // Seed a podcast + a content-rich episode: HTML description (exercises
    // clean_html), chapters (exercises the chapter mapping), and a transcript
    // (exercises store.transcript_for). All are derived fields the original
    // slice-local builder dropped.
    let feed = "https://example.com/feed.xml";
    let pid = PodcastId::new(Uuid::parse_str("a1a1a1a1-b2b2-c3c3-d4d4-e5e5e5e5e5e5").unwrap());
    let podcast = Podcast {
        id: pid,
        feed_url: Some(Url::parse(feed).unwrap()),
        title: "Queue Show".to_owned(),
        author: "Queue Author".to_owned(),
        image_url: None,
        description: "Show desc.".to_owned(),
        language: None,
        categories: vec!["Technology".to_owned()],
        discovered_at: Utc.timestamp_opt(1_704_067_200, 0).unwrap(),
        owner_pubkey_hex: None,
        nostr_visibility: podcast_core::NostrVisibility::Private,
        nostr_coordinate: None,
        title_is_placeholder: false,
        last_refreshed_at: None,
        etag: None,
        last_modified: None,
    };

    let mut ep = Episode::new(
        pid,
        feed,
        "queue-guid-001",
        "Queued Episode",
        Url::parse("https://example.com/audio/ep1.mp3").unwrap(),
        Utc.timestamp_opt(1_704_067_200, 0).unwrap(),
    );
    // Non-empty HTML description → clean_html → key emitted (the core regression).
    ep.description = "<p>An <b>HTML</b> description with markup.</p>".to_owned();
    ep.duration_secs = Some(1800.0);
    ep.position_secs = 120.0;
    let mut chapter = Chapter::new("Intro", 0.0);
    chapter.end_secs = Some(60.0);
    ep.chapters = Some(vec![chapter]);

    let ep_id = ep.id.0.to_string();

    {
        let mut s = handle.state.library.store.lock().unwrap();
        s.subscribe(podcast, vec![ep]);
        // Transcript text → store.transcript_for → key emitted.
        s.set_transcript(ep_id.clone(), "transcribed words here".to_owned());
    }

    // Enqueue using the LOWERCASE id (matches the canonical stored id that the
    // old resolve_queue_rows path matches against).
    {
        let mut q = handle.state.playback.queue.lock().unwrap();
        q.add_to_end(&ep_id);
    }

    // Slice-local queue rows.
    let playback_payload = build_playback_payload(&handle);
    let slice_queue = &playback_payload["queue"];

    // Full-snapshot queue rows.
    let full = build_podcast_update(&handle);
    let full_queue = serde_json::to_value(&full.queue).unwrap();

    assert_eq!(
        slice_queue, &full_queue,
        "slice-local queue row must be byte-identical to the build_podcast_update \
         queue row for a content-rich episode (description/chapters/transcript)"
    );

    // Sanity: the row really does carry the derived content (guards against a
    // future change that makes BOTH paths drop the fields and still pass above).
    let row = slice_queue
        .as_array()
        .and_then(|a| a.first())
        .expect("queue must contain the enqueued episode");
    assert!(
        row.get("description").is_some(),
        "queue row must carry the cleaned description; row: {row}"
    );
    assert!(
        row.get("chapters")
            .and_then(|c| c.as_array())
            .map(|a| !a.is_empty())
            .unwrap_or(false),
        "queue row must carry chapters; row: {row}"
    );
    assert!(
        row.get("transcript").is_some(),
        "queue row must carry the transcript; row: {row}"
    );
    assert_eq!(
        row.get("id").and_then(|v| v.as_str()),
        Some(ep_id.as_str()),
        "queue row id must be the LOWERCASE stored id, matching the library path"
    );

    drop(handle);
    unsafe { drop(Box::from_raw(app)) };
}
