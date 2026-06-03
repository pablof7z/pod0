//! Tests for [`super`] — owned-podcast create/update/delete lifecycle.
//!
//! Uses a NULL `app` pointer (no capability dispatch), so publish/relay
//! paths report `"signed"`/`"skipped"` rather than `"published"` — the
//! store + key + state mutations are what these tests exercise.

use super::*;
use crate::agent_handler::AgentChatHandler;
use crate::download::DownloadQueue;
use crate::host_op_publish::{create_owned, publish_show};
use crate::player::PlayerActor;
use crate::queue::PlaybackQueue;
use crate::store::identity::IdentityStore;
use crate::store::{PodcastKeyStore, PodcastStore};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};

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
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
        Arc::new(Mutex::new(None)),
        Arc::new(Mutex::new(Vec::new())),
    )
}

#[test]
fn register_synthetic_episode_inserts_into_kernel_store() {
    use crate::ffi::actions::publish_module::SyntheticChapterArg;

    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let handler = handler_with_store(store.clone());
    let pid = uuid::Uuid::new_v4().to_string();

    // Parent synthetic podcast must exist first.
    let created = create_synthetic(
        &handler,
        pid.clone(),
        "Agent Generated".into(),
        String::new(),
        "Agent".into(),
        None,
        None,
        vec![],
        Some("public".into()),
    );
    assert_eq!(created["ok"], true);
    let rev_before = handler.rev.load(std::sync::atomic::Ordering::Relaxed);

    let eid = uuid::Uuid::new_v4().to_string();
    let out = register_synthetic_episode(
        &handler,
        pid.clone(),
        eid.clone(),
        "Episode One".into(),
        "/tmp/agent-ep.m4a".into(),
        Some(42.0),
        vec![SyntheticChapterArg {
            start_secs: 0.0,
            title: "Intro".into(),
            image_url: None,
            source_episode_id: None,
        }],
        Some("the transcript".into()),
    );
    assert_eq!(out["ok"], true);
    assert_eq!(out["episode_id"], eid);
    assert!(
        handler.rev.load(std::sync::atomic::Ordering::Relaxed) > rev_before,
        "rev must bump so the projection picks up the episode"
    );

    let guard = store.lock().unwrap();
    let pod_id = podcast_core::PodcastId(uuid::Uuid::parse_str(&pid).unwrap());
    let eps = guard.episodes_for(pod_id);
    assert_eq!(eps.len(), 1);
    assert_eq!(eps[0].title, "Episode One");
    assert_eq!(guard.transcript_for(&eid), Some("the transcript"));
}

#[test]
fn register_synthetic_episode_fails_when_podcast_missing() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let handler = handler_with_store(store);
    let out = register_synthetic_episode(
        &handler,
        uuid::Uuid::new_v4().to_string(),
        uuid::Uuid::new_v4().to_string(),
        "Orphan".into(),
        "/tmp/x.m4a".into(),
        None,
        vec![],
        None,
    );
    assert_eq!(out["ok"], false);
}

#[test]
fn create_synthetic_inserts_row_then_create_owned_succeeds() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let handler = handler_with_store(store.clone());
    let id = uuid::Uuid::new_v4().to_string();

    // Before: create_owned fails because the row does not exist.
    let pre = create_owned(&handler, id.clone());
    assert_eq!(pre["ok"], false, "create_owned must fail with no row");

    // create_synthetic inserts the row.
    let out = create_synthetic(
        &handler,
        id.clone(),
        "Synthetic Show".into(),
        "A show".into(),
        "Agent".into(),
        Some("https://img/a.png".into()),
        None,
        vec!["Tech".into()],
        Some("public".into()),
    );
    assert_eq!(out["ok"], true);
    assert!(store.lock().unwrap().podcast_by_id_str(&id).is_some());

    // Now create_owned succeeds and stamps the owner pubkey.
    let post = create_owned(&handler, id.clone());
    assert_eq!(post["ok"], true);
    assert_eq!(
        post["pubkey_hex"].as_str().map(str::len),
        Some(64),
        "owner pubkey derived"
    );
}

#[test]
fn create_synthetic_rejects_bad_uuid() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let handler = handler_with_store(store);
    let out = create_synthetic(
        &handler,
        "not-a-uuid".into(),
        "T".into(),
        String::new(),
        String::new(),
        None,
        None,
        vec![],
        None,
    );
    assert_eq!(out["ok"], false);
}

#[test]
fn update_owned_mutates_metadata_and_skips_publish_when_private() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let id = uuid::Uuid::new_v4().to_string();
    {
        let mut s = store.lock().unwrap();
        s.insert_synthetic_podcast(
            &id,
            "Old".into(),
            "old desc".into(),
            "Agent".into(),
            None,
            None,
            vec![],
            podcast_core::NostrVisibility::Private,
        );
        s.set_nostr_enabled(true);
    }
    let handler = handler_with_store(store.clone());

    let out = update_owned(
        &handler,
        id.clone(),
        Some("New Title".into()),
        Some("new desc".into()),
        None,
        None,
        None,
    );
    assert_eq!(out["ok"], true);
    // Private → republish skipped even though nostr is enabled.
    assert_eq!(out["status"], "skipped");

    let s = store.lock().unwrap();
    let p = s.podcast_by_id_str(&id).unwrap();
    assert_eq!(p.title, "New Title");
    assert_eq!(p.description, "new desc");
}

#[test]
fn update_owned_persists_author_and_visibility_flip_republishes() {
    // Anti-clobber: author + visibility land on the kernel row (SSOT) so a
    // later snapshot push won't revert them. A private→public flip in the
    // same op flips the gate and republishes the show.
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let id = uuid::Uuid::new_v4().to_string();
    {
        let mut s = store.lock().unwrap();
        s.insert_synthetic_podcast(
            &id,
            "Flip Show".into(),
            "d".into(),
            "Old Author".into(),
            None,
            None,
            vec![],
            podcast_core::NostrVisibility::Private,
        );
        s.set_nostr_enabled(true);
    }
    let handler = handler_with_store(store.clone());
    create_owned(&handler, id.clone());

    let out = update_owned(
        &handler,
        id.clone(),
        None,
        None,
        Some("New Author".into()),
        None,
        Some("public".into()),
    );
    assert_eq!(out["ok"], true);
    // Visibility applied before the gate → republished in the same op.
    assert_eq!(out["status"], "republished");

    let s = store.lock().unwrap();
    let p = s.podcast_by_id_str(&id).unwrap();
    assert_eq!(p.author, "New Author");
    assert_eq!(p.nostr_visibility, podcast_core::NostrVisibility::Public);
}

#[test]
fn update_owned_republishes_when_public_and_nostr_enabled() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let id = uuid::Uuid::new_v4().to_string();
    {
        let mut s = store.lock().unwrap();
        s.insert_synthetic_podcast(
            &id,
            "Public Show".into(),
            "desc".into(),
            "Agent".into(),
            None,
            None,
            vec![],
            podcast_core::NostrVisibility::Public,
        );
        s.set_nostr_enabled(true);
    }
    let handler = handler_with_store(store.clone());
    // Claim the key so publish_show can sign.
    create_owned(&handler, id.clone());

    let out = update_owned(
        &handler,
        id.clone(),
        Some("Renamed".into()),
        None,
        None,
        None,
        None,
    );
    assert_eq!(out["ok"], true);
    assert_eq!(out["status"], "republished");
    // The nested publish result signed an event (null app → "signed").
    assert_eq!(out["publish"]["ok"], true);
    assert_eq!(store.lock().unwrap().podcast_by_id_str(&id).unwrap().title, "Renamed");
}

#[test]
fn update_owned_returns_error_for_unknown_podcast() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let handler = handler_with_store(store);
    let out = update_owned(&handler, "nope".into(), Some("x".into()), None, None, None, None);
    assert_eq!(out["ok"], false);
}

#[test]
fn delete_owned_removes_row_key_and_state() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let id = uuid::Uuid::new_v4().to_string();
    {
        let mut s = store.lock().unwrap();
        s.insert_synthetic_podcast(
            &id,
            "Doomed".into(),
            "d".into(),
            "Agent".into(),
            None,
            None,
            vec![],
            podcast_core::NostrVisibility::Public,
        );
        s.set_nostr_enabled(true);
    }
    let handler = handler_with_store(store.clone());
    create_owned(&handler, id.clone());
    // Publish a show so there is a stamped event id to NIP-09-delete.
    publish_show(&handler, id.clone());
    assert!(handler.podcast_keys.lock().unwrap().get_key(&id).is_some());

    let out = delete_owned(&handler, id.clone());
    assert_eq!(out["ok"], true);
    // Row gone.
    assert!(store.lock().unwrap().podcast_by_id_str(&id).is_none());
    // Key dropped.
    assert!(handler.podcast_keys.lock().unwrap().get_key(&id).is_none());
    // Publish state discarded.
    assert!(handler.publish_state.lock().unwrap().get(&id).is_none());
    // A NIP-09 deletion was signed (null app → relay "signed").
    assert!(out["deletion_event_id"].is_string());
}

#[test]
fn delete_owned_with_no_published_show_skips_nip09_but_tears_down() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let id = uuid::Uuid::new_v4().to_string();
    store.lock().unwrap().insert_synthetic_podcast(
        &id,
        "NeverPublished".into(),
        String::new(),
        String::new(),
        None,
        None,
        vec![],
        podcast_core::NostrVisibility::Public,
    );
    let handler = handler_with_store(store.clone());
    create_owned(&handler, id.clone());

    let out = delete_owned(&handler, id.clone());
    assert_eq!(out["ok"], true);
    assert_eq!(out["deletion_status"], "skipped");
    assert!(store.lock().unwrap().podcast_by_id_str(&id).is_none());
    assert!(handler.podcast_keys.lock().unwrap().get_key(&id).is_none());
}
