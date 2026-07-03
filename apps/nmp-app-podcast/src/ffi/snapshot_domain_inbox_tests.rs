//! Focused inbox fields carried by the `podcast.library` domain sidecar.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::{TimeZone, Utc};
use podcast_core::{Episode, Podcast};

use super::{register_domain_projections, SCHEMA_LIBRARY};
use crate::ffi::handle::PodcastHandle;
use crate::state::{Infra, PodcastAppState};
use crate::store::PodcastStore;

fn make_test_handle_with_app(app: *mut nmp_native_runtime::NmpApp) -> Box<PodcastHandle> {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let state = Arc::new(PodcastAppState::new(Infra::for_test(), store));
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

#[test]
fn library_domain_projects_inbox_last_triaged_at() {
    let app = Box::into_raw(Box::new(nmp_native_runtime::new_app()));
    assert!(!app.is_null(), "NmpApp allocation must succeed");
    let app_ref = unsafe { &*app };

    let handle = Arc::new(*make_test_handle_with_app(app));
    {
        let mut store = handle.state.library.store.lock().unwrap();
        let podcast = Podcast::new("Inbox Show");
        let podcast_id = podcast.id;
        let episode = Episode::new(
            podcast_id,
            "https://example.com/feed.xml",
            "guid-1",
            "Inbox Episode",
            url::Url::parse("https://example.com/ep.mp3").unwrap(),
            Utc.timestamp_opt(1_717_200_000, 0).unwrap(),
        );
        store.subscribe(podcast, vec![episode]);
    }
    handle.state.inbox.triage_cache.lock().unwrap().insert(
        "ep-ready".to_owned(),
        crate::inbox_llm::TriageResult::ready(0.8, "Ready".into(), vec![], 1_717_200_123),
    );

    register_domain_projections(app_ref, &handle);

    let projections = app_ref.run_typed_snapshot_projections();
    let lib = projections
        .iter()
        .find(|p| p.schema_id == SCHEMA_LIBRARY)
        .expect("library sidecar must be emitted on initial run");
    let val: serde_json::Value = serde_json::from_slice(&lib.payload).unwrap();
    assert_eq!(
        val["inbox_last_triaged_at"],
        serde_json::json!(1_717_200_123)
    );

    drop(handle);
    unsafe { drop(Box::from_raw(app)) };
}
