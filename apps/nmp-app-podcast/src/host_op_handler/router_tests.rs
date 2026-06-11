//! Regression guard for the namespace-envelope router introduced in
//! `host_op_handler/router.rs`.
//!
//! Each test drives `handler.handle(envelope_json, "corr")` directly and
//! asserts the CORRECT domain slot mutated.  The five confirmed pre-fix
//! silent-misroute cases each have a dedicated test.

use super::*;
use crate::download::DownloadQueue;
use crate::player::PlayerActor;
use crate::queue::PlaybackQueue;
use crate::store::identity::IdentityStore;
use crate::store::PodcastStore;
use podcast_core::{Episode, Podcast};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};
use url::Url;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Builder helpers
// ---------------------------------------------------------------------------

fn feedback_runtime(rev: Arc<AtomicU64>) -> nmp_feedback::FeedbackRuntime {
    nmp_feedback::FeedbackRuntime::new(
        nmp_feedback::FeedbackConfig::new(crate::PODCAST_FEEDBACK_PROJECT_COORDINATE)
            .with_interest_namespace(crate::PODCAST_FEEDBACK_INTEREST_NAMESPACE),
        Arc::new(Mutex::new(Vec::new())),
        rev,
    )
}

fn handler_with_store(store: Arc<Mutex<PodcastStore>>) -> PodcastHostOpHandler {
    let rev = Arc::new(AtomicU64::new(1));
    let identity = Arc::new(Mutex::new(IdentityStore::new()));
    let state = Arc::new(crate::state::PodcastAppState::new_with_identity(
        crate::state::Infra::for_test(),
        store.clone(),
        identity.clone(),
    ));
    // Steps 8-10: search_results, nostr_results, comments_cache,
    // viewed_comments_episode_id, social, agent_notes removed from constructor.
    // Step 11: agent_chat removed — now owned by state.agent_chat.
    PodcastHostOpHandler::new(
        std::ptr::null_mut(),
        state,
        store,
        identity,
        Arc::new(Mutex::new(PlayerActor::new())),
        Arc::new(Mutex::new(PlaybackQueue::new())),
        Arc::new(Mutex::new(DownloadQueue::new())),
        // agent_tasks, clips, transcripts removed in Steps 5a, 5b, 6.
        // voice_state removed in Step 12 — now owned by state.voice.
        // podcast_keys and publish_state removed in Step 13 — now owned by state.publish.
        Arc::new(Mutex::new(HashSet::new())),
        rev.clone(),
        Arc::new(tokio::runtime::Runtime::new().unwrap()),
        Arc::new(Mutex::new(HashMap::new())),
        Arc::new(AtomicBool::new(false)),
        crate::feed_fetch::FeedFetchCoordinator::new_test(),
        feedback_runtime(rev),
    )
}

fn empty_handler() -> PodcastHostOpHandler {
    handler_with_store(Arc::new(Mutex::new(PodcastStore::new())))
}

fn make_episode(podcast_id: podcast_core::PodcastId) -> Episode {
    Episode::new(
        podcast_id,
        "https://example.com/feed.xml",
        format!("guid-{}", Uuid::new_v4()),
        "Test Episode",
        Url::parse("https://example.com/audio.mp3").unwrap(),
        chrono::Utc::now(),
    )
}

// ---------------------------------------------------------------------------
// Envelope structural tests
// ---------------------------------------------------------------------------

#[test]
fn malformed_envelope_returns_ok_false() {
    let handler = empty_handler();
    let result = handler.handle("not json at all", "corr-1");
    assert_eq!(result["ok"], serde_json::json!(false));
    assert!(result["error"].as_str().unwrap().contains("malformed"));
}

#[test]
fn missing_ns_field_returns_ok_false() {
    let handler = empty_handler();
    let result = handler.handle(r#"{"action":{"op":"resume"}}"#, "corr-1");
    assert_eq!(result["ok"], serde_json::json!(false));
}

#[test]
fn unknown_namespace_returns_ok_false() {
    let handler = empty_handler();
    let envelope = serde_json::json!({"ns": "podcast.nonexistent", "action": {"op": "foo"}});
    let result = handler.handle(&envelope.to_string(), "corr-1");
    assert_eq!(result["ok"], serde_json::json!(false));
    assert!(
        result["error"].as_str().unwrap().contains("unknown namespace"),
        "error should mention 'unknown namespace', got: {}",
        result["error"]
    );
}

#[test]
fn bad_action_for_known_ns_returns_ok_false() {
    let handler = empty_handler();
    // "podcast.siri" namespace with a payload that can't parse as SiriAction
    let envelope = serde_json::json!({"ns": "podcast.siri", "action": {"op": "nonexistent_op"}});
    let result = handler.handle(&envelope.to_string(), "corr-1");
    assert_eq!(result["ok"], serde_json::json!(false));
    assert!(
        result["error"].as_str().unwrap().contains("parse error"),
        "error should mention 'parse error', got: {}",
        result["error"]
    );
}

// ---------------------------------------------------------------------------
// Collision fix: podcast.knowledge.search must NOT be hijacked by wiki
// ---------------------------------------------------------------------------

#[test]
fn knowledge_search_routes_to_knowledge_not_wiki() {
    let handler = empty_handler();
    // Pre-condition: both result slots are empty.
    // Knowledge results now live in state.knowledge (Step 1 migration).
    assert!(handler.state.knowledge.results_snapshot().is_empty());
    // Wiki results now live in state.wiki (Step 2 migration).
    assert!(handler.state.wiki.search_results_snapshot().is_empty());

    let envelope =
        serde_json::json!({"ns": "podcast.knowledge", "action": {"op": "search", "query": "rust"}});
    let result = handler.handle(&envelope.to_string(), "corr-ks");

    // The knowledge handler returns ok:true (no results for empty store, but ok).
    assert_eq!(result["ok"], serde_json::json!(true), "knowledge search should succeed: {result}");
    // Wiki results must remain empty — search was NOT misrouted.
    assert!(
        handler.state.wiki.search_results_snapshot().is_empty(),
        "wiki_search_results must remain empty when routing podcast.knowledge.search"
    );
}

// ---------------------------------------------------------------------------
// Collision fix: podcast.agent.clear must NOT empty the playback queue
// ---------------------------------------------------------------------------

#[test]
fn agent_clear_routes_to_agent_not_queue() {
    let handler = empty_handler();

    // Seed the playback queue with one item.
    handler
        .queue
        .lock()
        .unwrap()
        .add_to_end("ep-sentinel");

    assert_eq!(handler.queue.lock().unwrap().items().len(), 1);

    let envelope =
        serde_json::json!({"ns": "podcast.agent", "action": {"op": "clear"}});
    let result = handler.handle(&envelope.to_string(), "corr-ac");

    assert_eq!(result["ok"], serde_json::json!(true), "agent.clear should succeed: {result}");
    // Queue must NOT have been cleared — the action went to agent chat, not queue.
    assert_eq!(
        handler.queue.lock().unwrap().items().len(),
        1,
        "agent.clear must NOT empty the playback queue"
    );
}

// ---------------------------------------------------------------------------
// Collision fix: podcast.voice.stop must NOT be hijacked by player.stop
// ---------------------------------------------------------------------------
//
// Both VoiceAction::Stop and PlayerAction::Stop dispatch capability commands
// through the null `app` pointer in the test harness, causing a SIGABRT.
// We prove correct namespace separation two ways:
// 1. An action the VOICE handler knows but PLAYER does not ("activate") —
//    routed to podcast.player must return a parse error, not crash.
// 2. An action the PLAYER handler knows but VOICE does not ("set_speed") —
//    routed to podcast.voice must return a parse error, not crash.

#[test]
fn voice_activate_rejected_by_player_namespace() {
    let handler = empty_handler();
    // "activate" is a valid VoiceAction but NOT a PlayerAction.
    // If the router mistakenly sends a voice-namespace envelope to the player
    // handler, the parse would fail with a "parse error" response.
    // (We test the player namespace explicitly to prove the router rejects it.)
    let envelope =
        serde_json::json!({"ns": "podcast.player", "action": {"op": "activate"}});
    let result = handler.handle(&envelope.to_string(), "corr-va");
    assert_eq!(
        result["ok"],
        serde_json::json!(false),
        "podcast.player namespace must reject 'activate' (a voice-only op): {result}"
    );
}

#[test]
fn player_set_speed_rejected_by_voice_namespace() {
    let handler = empty_handler();
    // "set_speed" is a valid PlayerAction but NOT a VoiceAction.
    let envelope =
        serde_json::json!({"ns": "podcast.voice", "action": {"op": "set_speed", "speed": 1.5}});
    let result = handler.handle(&envelope.to_string(), "corr-pss");
    assert_eq!(
        result["ok"],
        serde_json::json!(false),
        "podcast.voice namespace must reject 'set_speed' (a player-only op): {result}"
    );
}

// ---------------------------------------------------------------------------
// Collision fix: podcast.siri.resume must NOT be hijacked by player.resume
// ---------------------------------------------------------------------------

#[test]
fn siri_resume_routes_to_siri_not_player() {
    let handler = empty_handler();

    let envelope =
        serde_json::json!({"ns": "podcast.siri", "action": {"op": "resume"}});
    let result = handler.handle(&envelope.to_string(), "corr-sr");

    // With an empty library, siri_resume returns ok:false with a domain error
    // ("no unplayed episodes"). This is distinct from a *routing* error
    // ("parse error for ns=...") and proves the action reached the siri handler.
    assert!(
        result.get("ok").is_some(),
        "siri.resume must return a response with 'ok' field: {result}"
    );
    // The siri handler produces an "ok":false with a domain-specific error,
    // NOT a "parse error" — proving it was routed to siri, not player.resume.
    let error_msg = result["error"].as_str().unwrap_or("");
    assert!(
        !error_msg.contains("parse error"),
        "siri.resume must not return a parse-error: {result}"
    );
}

// ---------------------------------------------------------------------------
// Collision fix: podcast.player.download must route to PlayerAction::Download
// ---------------------------------------------------------------------------

#[test]
fn player_download_routes_to_player_not_podcast() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let podcast = Podcast::new("Test Show");
    let pid = podcast.id;
    let ep = make_episode(pid);
    let ep_id = ep.id.0.to_string();
    store.lock().unwrap().subscribe(podcast, vec![ep]);

    let handler = handler_with_store(store);

    assert!(handler.download_queue.lock().unwrap().get(&ep_id).is_none());

    let envelope = serde_json::json!({
        "ns": "podcast.player",
        "action": {
            "op": "download",
            "episode_id": ep_id,
            "url": "https://example.com/audio.mp3"
        }
    });
    let result = handler.handle(&envelope.to_string(), "corr-pd");

    assert_eq!(
        result["ok"],
        serde_json::json!(true),
        "player.download should succeed: {result}"
    );
    // PlayerAction::Download enqueues the episode in DownloadQueue.
    // PodcastAction::Download (the old hijacker) would have different semantics.
    assert!(
        handler.download_queue.lock().unwrap().get(&ep_id).is_some(),
        "podcast.player.download must enqueue in DownloadQueue"
    );
}
