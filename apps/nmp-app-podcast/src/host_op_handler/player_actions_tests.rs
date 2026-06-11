//! Tests for [`super`] player actions — specifically the M1.3
//! "download-enqueue on play" business rule that moved out of Swift's
//! `PlaybackState.onEnsureDownloadEnqueued` callback (deleted in M1.5) and
//! into the Rust `handle_play` path. This is the Rust home of the coverage
//! the former `PlaybackStateAutoDownloadTests.swift` provided.

use super::*;
use crate::agent_handler::AgentChatHandler;
use crate::download::DownloadQueue;
use crate::ffi::actions::settings_module::SettingsAction;
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
        rev.clone(),
        Arc::new(Mutex::new(PodcastKeyStore::new())),
        Arc::new(Mutex::new(HashMap::new())),
        agent_chat,
        Arc::new(Mutex::new(HashMap::new())), // comments_cache
        Arc::new(Mutex::new(None::<String>)), // viewed_comments_episode_id
        Arc::new(tokio::runtime::Runtime::new().unwrap()),
        Arc::new(Mutex::new(HashMap::new())),
        Arc::new(AtomicBool::new(false)),
        Arc::new(Mutex::new(None)),
        Arc::new(Mutex::new(Vec::new())),
        crate::feed_fetch::FeedFetchCoordinator::new_test(),
        feedback_runtime(rev),
    )
}

fn feedback_runtime(rev: Arc<AtomicU64>) -> nmp_feedback::FeedbackRuntime {
    nmp_feedback::FeedbackRuntime::new(
        nmp_feedback::FeedbackConfig::new(crate::PODCAST_FEEDBACK_PROJECT_COORDINATE)
            .with_interest_namespace(crate::PODCAST_FEEDBACK_INTEREST_NAMESPACE),
        Arc::new(Mutex::new(Vec::new())),
        rev,
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
    assert_eq!(
        after_role,
        after_add + 1,
        "set_relay_role companion must bump rev"
    );

    handler.handle_settings_action(SettingsAction::RemoveRelay {
        url: "wss://relay.example".into(),
    });
    let after_remove = handler.rev.load(std::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        after_remove,
        after_role + 1,
        "remove_relay companion must bump rev"
    );
}

// ---- podcast.player queue-op routing (player-actor-queue-unification) -------
//
// The `podcast.player` `Enqueue`/`Dequeue`/`ClearQueue`/`PlayNext` ops are
// aliases for the canonical `PlaybackQueue` (`handle.queue`) — the same queue
// the snapshot's `Up Next` projection renders and `maybe_auto_advance` pops.
// These tests pin that the ops mutate `handler.queue`, NOT a separate per-actor
// queue (which no longer exists), closing the read/write split where the new
// app's Up Next swipe enqueued episodes the Up Next sheet never showed.

/// Seed a store with one subscribed podcast carrying `titles.len()` episodes;
/// returns the handler plus the episode ids in subscription order.
fn handler_with_episodes(titles: &[&str]) -> (PodcastHostOpHandler, Vec<String>) {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let podcast = Podcast::new("Queue Show");
    let pid = podcast.id;
    let episodes: Vec<Episode> = titles.iter().map(|t| make_episode(pid, t)).collect();
    let ids: Vec<String> = episodes.iter().map(|e| e.id.0.to_string()).collect();
    store.lock().unwrap().subscribe(podcast, episodes);
    (handler_with_store(store), ids)
}

#[test]
fn enqueue_op_appends_to_canonical_playback_queue() {
    let (handler, ids) = handler_with_episodes(&["A", "B"]);

    let r1 = handler.handle_player_action(
        PlayerAction::Enqueue {
            episode_id: ids[0].clone(),
        },
        "corr-enq-1",
    );
    let r2 = handler.handle_player_action(
        PlayerAction::Enqueue {
            episode_id: ids[1].clone(),
        },
        "corr-enq-2",
    );
    assert_eq!(r1["ok"], serde_json::json!(true));
    assert_eq!(r2["ok"], serde_json::json!(true));

    let q = handler.queue.lock().unwrap();
    assert_eq!(
        q.items(),
        &[ids[0].clone(), ids[1].clone()],
        "enqueue must append to the canonical PlaybackQueue, front-first"
    );
}

#[test]
fn enqueue_op_rejects_unknown_episode() {
    let (handler, _ids) = handler_with_episodes(&["A"]);
    let r = handler.handle_player_action(
        PlayerAction::Enqueue {
            episode_id: "ghost".into(),
        },
        "corr-enq-x",
    );
    assert_eq!(r["ok"], serde_json::json!(false));
    assert!(handler.queue.lock().unwrap().items().is_empty());
}

#[test]
fn dequeue_op_removes_from_canonical_queue() {
    let (handler, ids) = handler_with_episodes(&["A", "B"]);
    for id in &ids {
        let _ = handler.handle_player_action(
            PlayerAction::Enqueue {
                episode_id: id.clone(),
            },
            "corr-seed",
        );
    }
    let r = handler.handle_player_action(
        PlayerAction::Dequeue {
            episode_id: ids[0].clone(),
        },
        "corr-deq-1",
    );
    assert_eq!(r["ok"], serde_json::json!(true));
    assert_eq!(handler.queue.lock().unwrap().items(), &[ids[1].clone()]);
}

#[test]
fn clear_queue_op_empties_canonical_queue() {
    let (handler, ids) = handler_with_episodes(&["A", "B"]);
    for id in &ids {
        let _ = handler.handle_player_action(
            PlayerAction::Enqueue {
                episode_id: id.clone(),
            },
            "corr-seed",
        );
    }
    let r = handler.handle_player_action(PlayerAction::ClearQueue, "corr-clear");
    assert_eq!(r["ok"], serde_json::json!(true));
    assert!(handler.queue.lock().unwrap().items().is_empty());
}

#[test]
fn play_next_op_pops_canonical_queue_front() {
    let (handler, ids) = handler_with_episodes(&["A", "B"]);
    for id in &ids {
        let _ = handler.handle_player_action(
            PlayerAction::Enqueue {
                episode_id: id.clone(),
            },
            "corr-seed",
        );
    }
    let r = handler.handle_player_action(PlayerAction::PlayNext, "corr-next-1");
    assert_eq!(
        r["ok"],
        serde_json::json!(true),
        "play_next plays the front id"
    );
    // Front popped; the remaining entry stays queued.
    assert_eq!(handler.queue.lock().unwrap().items(), &[ids[1].clone()]);
}

#[test]
fn play_next_op_on_empty_queue_reports_error() {
    let (handler, _ids) = handler_with_episodes(&["A"]);
    let r = handler.handle_player_action(PlayerAction::PlayNext, "corr-next-empty");
    assert_eq!(r["ok"], serde_json::json!(false));
}

#[test]
fn advance_op_is_play_next_alias() {
    let (handler, ids) = handler_with_episodes(&["A", "B"]);
    for id in &ids {
        let _ = handler.handle_player_action(
            PlayerAction::Enqueue {
                episode_id: id.clone(),
            },
            "corr-seed",
        );
    }
    let r = handler.handle_player_action(PlayerAction::Advance, "corr-adv");
    assert_eq!(r["ok"], serde_json::json!(true));
    assert_eq!(handler.queue.lock().unwrap().items(), &[ids[1].clone()]);
}

#[test]
fn play_next_op_skips_stale_head() {
    let (handler, ids) = handler_with_episodes(&["A", "B"]);
    // Front is a stale id with no store entry; the valid second entry must
    // still play rather than strand behind the orphan.
    {
        let mut q = handler.queue.lock().unwrap();
        q.add_to_end("stale-orphan");
        q.add_to_end(&ids[1]);
    }
    let r = handler.handle_player_action(PlayerAction::PlayNext, "corr-stale");
    assert_eq!(
        r["ok"],
        serde_json::json!(true),
        "play_next must skip the unresolvable head and play the next valid entry"
    );
    assert!(
        handler.queue.lock().unwrap().items().is_empty(),
        "both the stale head and the played id are popped"
    );
    let _ = &ids[0];
}
