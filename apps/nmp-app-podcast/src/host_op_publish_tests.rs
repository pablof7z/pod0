//! Tests for [`super::host_op_publish`] — create-owned / publish-show / author-claim coverage.
//!
//! Extracted from `host_op_publish.rs` to keep that file under the 500-line hard limit.

use super::*;
use crate::agent_handler::AgentChatHandler;
use crate::download::DownloadQueue;
use crate::player::PlayerActor;
use crate::queue::PlaybackQueue;
use crate::store::identity::IdentityStore;
use crate::store::{PodcastKeyStore, PodcastStore};
use podcast_core::Podcast;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};

/// Construct a `PodcastHostOpHandler` with a NULL `app` pointer
/// — the publish handlers never dispatch capabilities, so the
/// pointer is never read. All other slots are initialized with the
/// same defaults `ffi::register::nmp_app_podcast_register` uses, so
/// the handler is fully wired even though only the publish path is
/// exercised here.
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
        Arc::new(Mutex::new(None)),
        Arc::new(Mutex::new(PlaybackQueue::new())),
        Arc::new(Mutex::new(DownloadQueue::new())),
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(Mutex::new(Vec::new())),
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
    )
}

#[test]
fn create_owned_then_publish_show_round_trip() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    // Seed the store with one podcast.
    let podcast = Podcast::new("Round-Trip Show");
    let podcast_id = podcast.id.0.to_string();
    store.lock().unwrap().subscribe(podcast, vec![]);

    let handler = handler_with_store(store.clone());

    // Step 1: create_owned_podcast → returns a pubkey and stamps it on the row.
    let out = create_owned(&handler, podcast_id.clone());
    assert_eq!(out["ok"], true);
    let pubkey = out["pubkey_hex"].as_str().expect("pubkey present").to_owned();
    assert_eq!(pubkey.len(), 64);
    // The podcast row now carries the owner pubkey.
    let stored_pk = store
        .lock()
        .unwrap()
        .podcast_by_id_str(&podcast_id)
        .and_then(|p| p.owner_pubkey_hex.clone())
        .expect("owner pubkey stamped");
    assert_eq!(stored_pk, pubkey);

    // Step 2: publish_show → returns a signed kind:10154 event with the same pubkey.
    // With a null app pointer relay dispatch is skipped, so status is "signed".
    let out2 = publish_show(&handler, podcast_id.clone());
    assert_eq!(out2["ok"], true);
    assert_eq!(out2["status"], "signed", "null-app pointer must yield status=signed");
    let tags = out2["event_tags"].as_array().expect("event_tags array");
    // NIP-F4 shows have no `d` tag — first tag is the title.
    assert_eq!(tags[0][0], "title");
    // The signer pubkey is threaded into the show tags via the "p" tag.
    let event: serde_json::Value =
        serde_json::from_str(out2["event_json"].as_str().unwrap()).unwrap();
    assert_eq!(event["kind"], 10154);
    assert_eq!(event["pubkey"], pubkey);
    // Real secp256k1 signing: id and sig must be non-null 64-char hex strings.
    let event_id = out2["event_id"].as_str().expect("event_id field present");
    assert_eq!(event_id.len(), 64, "event_id must be 64-char hex");
    let sig = event["sig"].as_str().expect("event.sig present");
    assert_eq!(sig.len(), 128, "sig must be 128-char hex");
    let id_in_event = event["id"].as_str().expect("event.id present");
    assert_eq!(id_in_event, event_id, "event_id in envelope matches event.id field");
}

#[test]
fn create_owned_rejects_unknown_podcast() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let handler = handler_with_store(store);
    let out = create_owned(&handler, "no-such-podcast".into());
    assert_eq!(out["ok"], false);
    assert!(out["error"].as_str().unwrap().contains("podcast not found"));
}

#[test]
fn publish_show_rejects_unowned_podcast() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let podcast = Podcast::new("Unclaimed");
    let pid = podcast.id.0.to_string();
    store.lock().unwrap().subscribe(podcast, vec![]);
    let handler = handler_with_store(store);
    // No create_owned first ⇒ no key ⇒ error.
    let out = publish_show(&handler, pid);
    assert_eq!(out["ok"], false);
    assert!(out["error"]
        .as_str()
        .unwrap()
        .contains("podcast not owned"));
}

#[test]
fn publish_author_claim_lists_every_owned_pubkey() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let p1 = Podcast::new("Show A");
    let p2 = Podcast::new("Show B");
    let id1 = p1.id.0.to_string();
    let id2 = p2.id.0.to_string();
    {
        let mut s = store.lock().unwrap();
        s.subscribe(p1, vec![]);
        s.subscribe(p2, vec![]);
    }
    let handler = handler_with_store(store);
    create_owned(&handler, id1);
    create_owned(&handler, id2);

    let out = publish_author_claim(&handler, "agent-pk-hex".into());
    assert_eq!(out["ok"], true);
    assert_eq!(out["owned_count"], 2);
    let tags = out["event_tags"].as_array().unwrap();
    assert_eq!(tags.len(), 2);
    for tag in tags {
        assert_eq!(tag[0], "p");
        assert_eq!(tag[1].as_str().unwrap().len(), 64);
    }
    let event: serde_json::Value =
        serde_json::from_str(out["event_json"].as_str().unwrap()).unwrap();
    assert_eq!(event["kind"], 10064);
    assert_eq!(event["pubkey"], "agent-pk-hex");
}

#[test]
fn publish_author_claim_rejects_empty_agent_pubkey() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let handler = handler_with_store(store);
    let out = publish_author_claim(&handler, String::new());
    assert_eq!(out["ok"], false);
}

#[test]
fn remove_owned_clears_key_and_pubkey_field() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let p = Podcast::new("Doomed");
    let id = p.id.0.to_string();
    store.lock().unwrap().subscribe(p, vec![]);
    let handler = handler_with_store(store.clone());
    create_owned(&handler, id.clone());

    let out = remove_owned(&handler, id.clone());
    assert_eq!(out["ok"], true);
    assert!(handler.podcast_keys.lock().unwrap().get_key(&id).is_none());
    assert!(store
        .lock()
        .unwrap()
        .podcast_by_id_str(&id)
        .and_then(|p| p.owner_pubkey_hex.clone())
        .is_none());
}
