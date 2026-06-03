//! Tests for [`super`] player actions — specifically the M1.3
//! "download-enqueue on play" business rule that moved out of Swift's
//! `PlaybackState.onEnsureDownloadEnqueued` callback (deleted in M1.5) and
//! into the Rust `handle_play` path. This is the Rust home of the coverage
//! the former `PlaybackStateAutoDownloadTests.swift` provided.

use super::*;
use crate::agent_handler::AgentChatHandler;
use crate::ffi::actions::settings_module::SettingsAction;
use crate::download::DownloadQueue;
use crate::player::PlayerActor;
use crate::queue::PlaybackQueue;
use crate::store::identity::IdentityStore;
use crate::store::{PodcastKeyStore, PodcastStore};
use podcast_core::{Episode, Podcast, PodcastId};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};
use url::Url;
use uuid::Uuid;

/// Construct a `PodcastHostOpHandler` with a NULL `app` pointer. `handle_play`
/// only dispatches a capability through `app` when there is a follow-up audio
/// command; the enqueue-on-play rule under test mutates the in-process
/// `download_queue` and never reads `app`. Mirrors the constructor defaults in
/// `ffi::register::nmp_app_podcast_register`.
fn handler_with_store(store: Arc<Mutex<PodcastStore>>) -> PodcastHostOpHandler {
    let rev = Arc::new(AtomicU64::new(1));
    let agent_chat = AgentChatHandler::new_without_runtime(
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
        rev.clone(),
    );
    PodcastHostOpHandler::new(
        std::ptr::null_mut(),
        store,
        Arc::new(Mutex::new(IdentityStore::new())),
        Arc::new(Mutex::new(PlayerActor::new())),
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(Mutex::new(PlaybackQueue::new())),
        Arc::new(Mutex::new(DownloadQueue::new())),
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(Mutex::new(podcast_knowledge::KnowledgeStore::new())),
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(Mutex::new(HashMap::new())),
        Arc::new(Mutex::new(HashSet::new())),
        Arc::new(Mutex::new(Default::default())),
        Arc::new(Mutex::new(HashMap::new())),
        rev,
        Arc::new(Mutex::new(PodcastKeyStore::new())),
        Arc::new(Mutex::new(HashMap::new())),
        agent_chat,
        Arc::new(Mutex::new(HashMap::new())),
        Arc::new(tokio::runtime::Runtime::new().unwrap()),
        Arc::new(Mutex::new(HashMap::new())),
        Arc::new(AtomicBool::new(false)),
        Arc::new(Mutex::new(None)),
        Arc::new(Mutex::new(Vec::new())),
    )
}

fn make_episode(podcast_id: PodcastId, title: &str) -> Episode {
    Episode::new(
        podcast_id,
        "https://example.com/feed.xml",
        format!("guid-{}", Uuid::new_v4()),
        title,
        Url::parse("https://example.com/audio.mp3").unwrap(),
        chrono::Utc::now(),
    )
}

/// Playing a not-yet-downloaded episode enqueues a background download in the
/// same dispatch — the rule formerly enforced by Swift's
/// `onEnsureDownloadEnqueued`. A freshly-subscribed episode has no local file,
/// so `episode_is_downloaded` is false and `handle_play` must enqueue it.
#[test]
fn play_enqueues_download_for_not_downloaded_episode() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let podcast = Podcast::new("Play Show");
    let pid = podcast.id;
    let ep = make_episode(pid, "Ep");
    let ep_id = ep.id.0.to_string();
    store.lock().unwrap().subscribe(podcast, vec![ep]);

    let handler = handler_with_store(store);
    let result = handler.handle_play(ep_id.clone(), "corr-play-1");

    assert_eq!(result["ok"], serde_json::json!(true));
    let dq = handler.download_queue.lock().unwrap();
    assert!(
        dq.get(&ep_id).is_some(),
        "playing a not-downloaded episode must enqueue it for download"
    );
}

/// The UI's play path dispatches `load` (not `play`), so `handle_load` must
/// also enqueue a background download for a streamed episode — otherwise
/// restored mini-player plays (which skip the Swift-side enqueue) would never
/// download.
#[test]
fn load_enqueues_download_for_not_downloaded_episode() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let podcast = Podcast::new("Load Show");
    let pid = podcast.id;
    let ep = make_episode(pid, "Ep");
    let ep_id = ep.id.0.to_string();
    store.lock().unwrap().subscribe(podcast, vec![ep]);

    let handler = handler_with_store(store);
    let result = handler.handle_load(ep_id.clone(), "corr-load-1");

    assert_eq!(result["ok"], serde_json::json!(true));
    let dq = handler.download_queue.lock().unwrap();
    assert!(
        dq.get(&ep_id).is_some(),
        "loading a not-downloaded episode must enqueue it for download"
    );
}

/// The enqueue is idempotent: replaying the same episode does not create a
/// second queue entry or leave the queue in an inconsistent state.
#[test]
fn replaying_same_episode_does_not_double_enqueue() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let podcast = Podcast::new("Replay Show");
    let pid = podcast.id;
    let ep = make_episode(pid, "Ep");
    let ep_id = ep.id.0.to_string();
    store.lock().unwrap().subscribe(podcast, vec![ep]);

    let handler = handler_with_store(store);
    let _ = handler.handle_play(ep_id.clone(), "corr-1");
    let _ = handler.handle_play(ep_id.clone(), "corr-2");

    let dq = handler.download_queue.lock().unwrap();
    assert!(dq.get(&ep_id).is_some());
    assert_eq!(
        dq.active_count() + dq.queued_count(),
        1,
        "replaying must not create a duplicate download entry"
    );
}

/// Relay-edit reactivity seam: the `DispatchHostOp` companion for an
/// `AddRelay`/`RemoveRelay`/`SetRelayRole` action MUST bump `handle.rev`.
/// Without it the rev-gated snapshot push frame would serve stale cached JSON
/// and iOS would dedupe the tick, so a relay edit would never reach the UI.
/// (The matching `ActorCommand::AddRelay`/`RemoveRelay` that mutates the
/// kernel `AppRelaySlot` is verified in `settings_module_tests.rs`.)
#[test]
fn relay_settings_actions_bump_rev() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let handler = handler_with_store(store);

    let before = handler.rev.load(std::sync::atomic::Ordering::Relaxed);
    handler.handle_settings_action(SettingsAction::AddRelay {
        url: "wss://relay.example".into(),
        role: "both".into(),
    });
    let after_add = handler.rev.load(std::sync::atomic::Ordering::Relaxed);
    assert_eq!(after_add, before + 1, "add_relay companion must bump rev");

    handler.handle_settings_action(SettingsAction::SetRelayRole {
        url: "wss://relay.example".into(),
        role: "read".into(),
    });
    let after_role = handler.rev.load(std::sync::atomic::Ordering::Relaxed);
    assert_eq!(after_role, after_add + 1, "set_relay_role companion must bump rev");

    handler.handle_settings_action(SettingsAction::RemoveRelay {
        url: "wss://relay.example".into(),
    });
    let after_remove = handler.rev.load(std::sync::atomic::Ordering::Relaxed);
    assert_eq!(after_remove, after_role + 1, "remove_relay companion must bump rev");
}
