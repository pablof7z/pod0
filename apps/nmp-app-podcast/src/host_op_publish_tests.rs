//! Tests for [`super::host_op_publish`] — create-owned / publish-show /
//! publish-episode / author-claim coverage.
//!
//! Extracted from `host_op_publish.rs` to keep that file under the 500-line
//! hard limit.
//!
//! ## What changed (D13 — retire app-side crypto)
//!
//! The old tests asserted that `publish_show` / `publish_episode` returned a
//! pre-signed `event_json` with a valid `sig` field, and that `resolve_episode_tags`
//! called an injected `fetch` closure for the Blossom upload.
//!
//! Under the new architecture:
//! - `publish_show` dispatches `PublishRaw { signer_pubkey: Some(pubkey_hex) }`
//!   to the kernel and returns `{ "status": "signed" }` (null app in tests)
//!   instead of a `event_json` blob. No app-side signing occurs.
//! - `publish_episode` dispatches `PublishRaw { signer_pubkey }` similarly.
//!   Blossom is dispatched through `nmp.blossom.upload` (null app → skipped).
//! - The response no longer carries `event_json` / `event_id` / `sig`.
//!
//! Tests are updated to assert the NEW contract (kernel-dispatch shape) and
//! explicitly assert that `event_json` / `sig` are NOT in the response (so a
//! regression back to app-side signing would be caught).

use super::*;
use crate::store::identity::IdentityStore;
use crate::store::PodcastStore;
use podcast_core::types::episode::Episode;
use podcast_core::Podcast;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};
use url::Url;
use chrono::Utc;

/// Construct a `PodcastHostOpHandler` with a NULL `app` pointer
/// — the publish handlers never dispatch capabilities, so the
/// pointer is never read. All other slots are initialized with the
/// same defaults `ffi::register::nmp_app_podcast_register` uses, so
/// the handler is fully wired even though only the publish path is
/// exercised here.
fn handler_with_store(store: Arc<Mutex<PodcastStore>>) -> PodcastHostOpHandler {
    let rev = Arc::new(AtomicU64::new(1));
    let identity = Arc::new(Mutex::new(IdentityStore::new()));
    // Step 16: feedback injected; feed_fetch + feedback removed from handler::new.
    let state = Arc::new(crate::state::PodcastAppState::new_with_identity(
        crate::state::Infra::for_test(),
        store.clone(),
        identity.clone(),
        feedback_runtime(rev.clone()),
    ));
    // Steps 8-N+1: all substates in PodcastAppState; new takes only (app, state).
    PodcastHostOpHandler::new(
        std::ptr::null_mut(),
        state,
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

// ---------------------------------------------------------------------------
// create_owned + publish_show round trip (D13 dispatch shape)
// ---------------------------------------------------------------------------

/// After create_owned the pubkey is stamped on the row; publish_show dispatches
/// `PublishRaw { signer_pubkey }` via the kernel (null-app → status="signed").
/// The response must NOT carry `event_json` or `sig` — those only existed when
/// the app signed locally. The new shape proves the kernel is the signer.
#[test]
fn create_owned_then_publish_show_dispatches_via_kernel() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let podcast = Podcast::new("Round-Trip Show");
    let podcast_id = podcast.id.0.to_string();
    store.lock().unwrap().subscribe(podcast, vec![]);

    let handler = handler_with_store(store.clone());

    // Step 1: create_owned_podcast → returns a pubkey and stamps it on the row.
    let out = create_owned(&handler, podcast_id.clone());
    assert_eq!(out["ok"], true, "create_owned failed: {out}");
    let pubkey = out["pubkey_hex"]
        .as_str()
        .expect("pubkey_hex present")
        .to_owned();
    assert_eq!(pubkey.len(), 64, "pubkey must be 64-char hex");

    // The podcast row now carries the owner pubkey.
    let stored_pk = store
        .lock()
        .unwrap()
        .podcast_by_id_str(&podcast_id)
        .and_then(|p| p.owner_pubkey_hex.clone())
        .expect("owner pubkey stamped on row");
    assert_eq!(stored_pk, pubkey);

    // Step 2: publish_show → kernel dispatch (null-app → "signed").
    let out2 = publish_show(&handler, podcast_id.clone());
    assert_eq!(out2["ok"], true, "publish_show failed: {out2}");
    assert_eq!(
        out2["status"], "signed",
        "null-app pointer must yield status=signed"
    );

    // The response must carry the per-podcast pubkey_hex (used by the kernel
    // to select the signer) and the event_tags used to build the event.
    assert_eq!(
        out2["pubkey_hex"].as_str().expect("pubkey_hex in response"),
        pubkey,
        "response pubkey_hex must match the registered per-podcast key"
    );
    let tags = out2["event_tags"].as_array().expect("event_tags array");
    assert_eq!(tags[0][0], "title", "first NIP-F4 show tag is title");

    // CRITICAL: no app-side signed event — event_json / sig / event_id must
    // NOT be in the response. A regression would re-add these.
    assert!(
        out2.get("event_json").is_none() || out2["event_json"].is_null(),
        "event_json must be absent from the kernel-dispatch response (app-side signing was deleted)"
    );
    assert!(
        out2.get("sig").is_none() || out2["sig"].is_null(),
        "sig must be absent — the kernel signs, not the app"
    );
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
    assert!(out["error"].as_str().unwrap().contains("podcast not owned"));
}

// ---------------------------------------------------------------------------
// publish_episode (D13 dispatch shape)
// ---------------------------------------------------------------------------

/// Build a minimal episode whose RSS enclosure points at a known URL.
fn test_episode_for_podcast(podcast: &Podcast) -> Episode {
    Episode::new(
        podcast.id.clone(),
        "https://feed.example/rss.xml",
        "guid-ep1",
        "Episode One",
        Url::parse("https://feed.example/enclosure.mp3").unwrap(),
        Utc::now(),
    )
}

/// publish_episode dispatches `PublishRaw { signer_pubkey }` via the kernel
/// (null-app → status="signed"). No app-side signing; response has pubkey_hex
/// but NOT event_json / sig.
#[test]
fn publish_episode_dispatches_via_kernel() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let podcast = Podcast::new("My Podcast");
    let podcast_id = podcast.id.0.to_string();
    let episode = test_episode_for_podcast(&podcast);
    let episode_id = episode.id.0.to_string();
    {
        let mut s = store.lock().unwrap();
        s.subscribe(podcast, vec![episode]);
    }

    let handler = handler_with_store(store.clone());

    // Must create_owned first to mint the per-podcast key.
    let owned_out = create_owned(&handler, podcast_id.clone());
    assert_eq!(owned_out["ok"], true);
    let pubkey = owned_out["pubkey_hex"].as_str().unwrap().to_owned();

    let out = handle_publish_action(
        &handler,
        PublishAction::PublishEpisode { episode_id: episode_id.clone() },
    );
    assert_eq!(out["ok"], true, "publish_episode failed: {out}");
    assert_eq!(
        out["status"], "signed",
        "null-app must yield status=signed"
    );

    // Response carries the per-podcast pubkey_hex (the signer the kernel will use).
    assert_eq!(
        out["pubkey_hex"].as_str().expect("pubkey_hex in response"),
        pubkey,
        "response pubkey_hex matches the per-podcast key"
    );

    // event_tags present (the kernel gets the unsigned tag set).
    let tags = out["event_tags"].as_array().expect("event_tags array");
    let audio_tag = tags
        .iter()
        .find(|t| t.get(0).and_then(|v| v.as_str()) == Some("audio"))
        .expect("audio tag present in episode tags");
    assert!(
        !audio_tag.get(1).and_then(|v| v.as_str()).unwrap_or("").is_empty(),
        "audio tag must have a URL"
    );

    // CRITICAL: no app-side signed event.
    assert!(
        out.get("event_json").is_none() || out["event_json"].is_null(),
        "event_json must be absent from the kernel-dispatch response"
    );
    assert!(
        out.get("sig").is_none() || out["sig"].is_null(),
        "sig must be absent — the kernel signs, not the app"
    );

    // blossom_correlation_id is null for a null-app (kernel not reached).
    assert!(
        out["blossom_correlation_id"].is_null(),
        "blossom_correlation_id must be null when app is null: {out}"
    );
}

#[test]
fn publish_episode_rejects_unowned_podcast() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let podcast = Podcast::new("Unowned Podcast");
    let episode = test_episode_for_podcast(&podcast);
    let episode_id = episode.id.0.to_string();
    store.lock().unwrap().subscribe(podcast, vec![episode]);

    let handler = handler_with_store(store);
    // No create_owned → no key → error.
    let out = handle_publish_action(
        &handler,
        PublishAction::PublishEpisode { episode_id },
    );
    assert_eq!(out["ok"], false);
    assert!(out["error"].as_str().unwrap().contains("podcast not owned"));
}

// ---------------------------------------------------------------------------
// publish_author_claim
// ---------------------------------------------------------------------------

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
    // event_json is not in the response — NMP builds and signs the event
    // via PublishRaw; status is "signed" (null app in unit tests).
    assert_eq!(out["status"], "signed");
}

#[test]
fn publish_author_claim_rejects_empty_agent_pubkey() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let handler = handler_with_store(store);
    let out = publish_author_claim(&handler, String::new());
    assert_eq!(out["ok"], false);
}

// ---------------------------------------------------------------------------
// remove_owned
// ---------------------------------------------------------------------------

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
    // Step 13: podcast_keys now in state.publish (PublishState).
    assert!(handler.state.publish.podcast_keys.lock().unwrap().get_key(&id).is_none());
    assert!(store
        .lock()
        .unwrap()
        .podcast_by_id_str(&id)
        .and_then(|p| p.owner_pubkey_hex.clone())
        .is_none());
}
