//! Tests for [`super`] — owned-podcast create/update/delete lifecycle.
//!
//! Uses a NULL `app` pointer (no capability dispatch), so publish/relay
//! paths report `"signed"`/`"skipped"` rather than `"published"` — the
//! store + key + state mutations are what these tests exercise.

use super::*;
use crate::host_op_publish::{create_owned, publish_show};
use crate::store::identity::IdentityStore;
use crate::store::PodcastStore;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

fn handler_with_store(store: Arc<Mutex<PodcastStore>>) -> PodcastHostOpHandler {
    let rev = Arc::new(AtomicU64::new(1));
    let identity = Arc::new(Mutex::new(IdentityStore::new()));
    // Step 16: feedback injected into PodcastAppState.
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

/// Seed a feed-less podcast row directly via the store (the `create_podcast`
/// host-op path is exercised in `podcast_actions` tests; here we just need a
/// row to exist for the owned-podcast lifecycle handlers under test).
fn seed_owned_row(
    store: &Arc<Mutex<PodcastStore>>,
    id: &str,
    author: &str,
    visibility: podcast_core::NostrVisibility,
) {
    let mut s = store.lock().unwrap();
    s.create_podcast(
        id,
        "Show".into(),
        "d".into(),
        author.into(),
        None,
        None,
        None,
        vec![],
        visibility,
        false,
    );
}

#[test]
fn create_owned_requires_an_existing_row() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let handler = handler_with_store(store.clone());
    let id = uuid::Uuid::new_v4().to_string();

    // Before: create_owned fails because the row does not exist.
    let pre = create_owned(&handler, id.clone());
    assert_eq!(pre["ok"], false, "create_owned must fail with no row");

    // Seed the row, then create_owned succeeds and stamps the owner pubkey.
    seed_owned_row(&store, &id, "Agent", podcast_core::NostrVisibility::Public);
    let post = create_owned(&handler, id.clone());
    assert_eq!(post["ok"], true);
    assert_eq!(
        post["pubkey_hex"].as_str().map(str::len),
        Some(64),
        "owner pubkey derived"
    );
}

#[test]
fn update_owned_mutates_metadata_and_skips_publish_when_private() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let id = uuid::Uuid::new_v4().to_string();
    {
        let mut s = store.lock().unwrap();
        s.create_podcast(
            &id,
            "Old".into(),
            "old desc".into(),
            "Agent".into(),
            None,
            None,
            None,
            vec![],
            podcast_core::NostrVisibility::Private,
            false,
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
        s.create_podcast(
            &id,
            "Flip Show".into(),
            "d".into(),
            "Old Author".into(),
            None,
            None,
            None,
            vec![],
            podcast_core::NostrVisibility::Private,
            false,
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
        s.create_podcast(
            &id,
            "Public Show".into(),
            "desc".into(),
            "Agent".into(),
            None,
            None,
            None,
            vec![],
            podcast_core::NostrVisibility::Public,
            false,
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
    assert_eq!(
        store.lock().unwrap().podcast_by_id_str(&id).unwrap().title,
        "Renamed"
    );
}

#[test]
fn update_owned_returns_error_for_unknown_podcast() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let handler = handler_with_store(store);
    let out = update_owned(
        &handler,
        "nope".into(),
        Some("x".into()),
        None,
        None,
        None,
        None,
    );
    assert_eq!(out["ok"], false);
}

#[test]
fn delete_owned_removes_row_key_and_state() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let id = uuid::Uuid::new_v4().to_string();
    {
        let mut s = store.lock().unwrap();
        s.create_podcast(
            &id,
            "Doomed".into(),
            "d".into(),
            "Agent".into(),
            None,
            None,
            None,
            vec![],
            podcast_core::NostrVisibility::Public,
            false,
        );
        s.set_nostr_enabled(true);
    }
    let handler = handler_with_store(store.clone());
    create_owned(&handler, id.clone());
    // Publish a show so there is a stamped event id to NIP-09-delete.
    publish_show(&handler, id.clone());
    assert!(handler.state.publish.podcast_keys.lock().unwrap().get_key(&id).is_some());

    let out = delete_owned(&handler, id.clone());
    assert_eq!(out["ok"], true);
    // Row gone.
    assert!(store.lock().unwrap().podcast_by_id_str(&id).is_none());
    // Key dropped.
    assert!(handler.state.publish.podcast_keys.lock().unwrap().get_key(&id).is_none());
    // Publish state discarded.
    // Step 13: publish_state now in state.publish (PublishState).
    assert!(handler.state.publish.publish_state.lock().unwrap().get(&id).is_none());
    // A NIP-09 deletion was dispatched via the kernel (null app → "signed").
    // The new architecture does NOT return deletion_event_id (the kernel signs
    // the event, not the app — so no event id is available at dispatch time).
    assert!(
        out.get("deletion_event_id").is_none() || out["deletion_event_id"].is_null(),
        "deletion_event_id must be absent — kernel handles signing (D13): {out}"
    );
    assert_eq!(
        out["deletion_status"].as_str().unwrap_or(""),
        "signed",
        "null-app deletion must report status=signed: {out}"
    );
}

#[test]
fn delete_owned_with_no_published_show_skips_nip09_but_tears_down() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let id = uuid::Uuid::new_v4().to_string();
    store.lock().unwrap().create_podcast(
        &id,
        "NeverPublished".into(),
        String::new(),
        String::new(),
        None,
        None,
        None,
        vec![],
        podcast_core::NostrVisibility::Public,
        false,
    );
    let handler = handler_with_store(store.clone());
    create_owned(&handler, id.clone());

    let out = delete_owned(&handler, id.clone());
    assert_eq!(out["ok"], true);
    assert_eq!(out["deletion_status"], "skipped");
    assert!(store.lock().unwrap().podcast_by_id_str(&id).is_none());
    assert!(handler.state.publish.podcast_keys.lock().unwrap().get_key(&id).is_none());
}

/// Private→public flip publishes the show event AND identifies all N episodes
/// for kind:54 backfill (D0: kernel owns all publish policy). The backfill is
/// self-enqueued as N separate `publish_episode` actions so the actor yields
/// between them (D8 — no synchronous upload loop on the actor thread). Checks
/// that `episodes_queued` (the policy decision) matches the seeded episode
/// count and that the show republish ran (inner `publish.ok`). With the null
/// app pointer the self-dispatch is a no-op so `episodes_accepted` is 0 — the
/// live-kernel fan-out is covered by the headless nipf4 scenario.
#[test]
fn private_to_public_flip_backfills_all_episodes_as_kind54() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let podcast_id = uuid::Uuid::new_v4().to_string();
    const EPISODE_COUNT: usize = 3;

    {
        let mut s = store.lock().unwrap();
        s.create_podcast(
            &podcast_id,
            "Private Show".into(),
            "d".into(),
            "Agent".into(),
            None,
            None,
            None,
            vec![],
            podcast_core::NostrVisibility::Private,
            false,
        );
        s.set_nostr_enabled(true);
        // Seed EPISODE_COUNT episodes with http enclosure URLs (no Blossom
        // upload path in tests — null app pointer skips the upload).
        for i in 0..EPISODE_COUNT {
            let eid = uuid::Uuid::new_v4().to_string();
            s.add_episode(
                &podcast_id,
                &eid,
                format!("Episode {i}"),
                &format!("https://example.com/ep{i}.mp3"),
                format!("desc {i}"),
                Some(120.0),
                None,
                vec![],
                None,
            );
        }
    }
    let handler = handler_with_store(store.clone());
    // Claim the per-podcast key so publish_show (and publish_episode) can sign.
    create_owned(&handler, podcast_id.clone());

    let out = update_owned(
        &handler,
        podcast_id.clone(),
        None,
        None,
        None,
        None,
        Some("public".into()),
    );

    assert_eq!(out["ok"], true, "update_owned must succeed");
    assert_eq!(out["status"], "republished", "visibility flip must trigger republish");
    // Show event published.
    assert_eq!(out["publish"]["ok"], true, "show event republish must succeed");
    // All episodes identified for backfill (one self-dispatched publish_episode
    // action each — the actor yields between them).
    assert_eq!(
        out["episodes_queued"].as_u64().unwrap_or(0),
        EPISODE_COUNT as u64,
        "all {EPISODE_COUNT} episodes must be queued for kind:54 backfill"
    );
    // Null app in tests → self-dispatch is a no-op → none accepted by the FFI
    // registry (the live fan-out is exercised by the headless scenario).
    assert_eq!(
        out["episodes_accepted"].as_u64().unwrap_or(99),
        0,
        "null app pointer must accept 0 self-dispatches"
    );
    // Post-state: visibility is public on the kernel row.
    assert_eq!(
        store
            .lock()
            .unwrap()
            .podcast_by_id_str(&podcast_id)
            .unwrap()
            .nostr_visibility,
        podcast_core::NostrVisibility::Public
    );
}

/// Already-public show update does NOT backfill episodes (not a flip).
#[test]
fn already_public_show_update_does_not_backfill_episodes() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let podcast_id = uuid::Uuid::new_v4().to_string();
    {
        let mut s = store.lock().unwrap();
        s.create_podcast(
            &podcast_id,
            "Public Show".into(),
            "d".into(),
            "Agent".into(),
            None,
            None,
            None,
            vec![],
            podcast_core::NostrVisibility::Public,
            false,
        );
        s.set_nostr_enabled(true);
        let eid = uuid::Uuid::new_v4().to_string();
        s.add_episode(
            &podcast_id,
            &eid,
            "Ep".into(),
            "https://example.com/ep.mp3",
            String::new(),
            None,
            None,
            vec![],
            None,
        );
    }
    let handler = handler_with_store(store.clone());
    create_owned(&handler, podcast_id.clone());

    let out = update_owned(
        &handler,
        podcast_id.clone(),
        Some("Renamed".into()),
        None,
        None,
        None,
        None, // no visibility change — stays public
    );
    assert_eq!(out["ok"], true);
    assert_eq!(out["status"], "republished");
    // No flip → no episodes queued for backfill.
    assert_eq!(
        out["episodes_queued"].as_u64().unwrap_or(99),
        0,
        "non-flip update must not queue any episodes for backfill"
    );
}

/// NIP-09 deletion tag coverage — the single kind:5 event MUST carry BOTH
/// `["k","10154"]` (show) and `["k","54"]` (episodes) so the whole per-podcast
/// footprint is tombstoned in one dispatch.
///
/// We verify two things:
///   1. The `deletion_tags()` helper (the authoritative tag source) contains
///      exactly the two required k-tags, in the right positions.
///   2. The live `delete_owned` path routes through the per-podcast kernel
///      signer (D13) — confirmed by `deletion_status == "signed"` on a
///      null-app handler that has a registered key AND a stamped publish record.
#[test]
fn delete_owned_nip09_covers_show_and_episodes() {
    use podcast_discovery::{KIND_EPISODE, KIND_SHOW};

    // ── Part 1: tag shape assertion (pure, no app needed) ─────────────────
    let tags = super::deletion_tags();
    assert_eq!(tags.len(), 2, "deletion must carry exactly two k-tags");

    let kind_values: Vec<&str> = tags.iter()
        .map(|t| {
            assert_eq!(t.len(), 2, "each tag must be [\"k\", \"<kind>\"]");
            assert_eq!(t[0], "k", "tag identifier must be \"k\"");
            t[1].as_str()
        })
        .collect();

    let show_str = KIND_SHOW.to_string();
    let episode_str = KIND_EPISODE.to_string();
    assert!(
        kind_values.contains(&show_str.as_str()),
        "deletion must include k:{KIND_SHOW} (show): {tags:?}"
    );
    assert!(
        kind_values.contains(&episode_str.as_str()),
        "deletion must include k:{KIND_EPISODE} (episodes): {tags:?}"
    );

    // ── Part 2: live delete_owned routes via per-podcast signer (D13) ─────
    // Seed a podcast, claim its key, stamp a publish record, then delete.
    // Null-app publish_raw_with_signer_via_nmp returns "signed" immediately,
    // but ONLY when the signer branch is reached (key + publish record both
    // present). This confirms the deletion is kernel-routed, not skipped.
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let id = uuid::Uuid::new_v4().to_string();
    {
        let mut s = store.lock().unwrap();
        s.create_podcast(
            &id,
            "Ephemeral Show".into(),
            "d".into(),
            "Agent".into(),
            None,
            None,
            None,
            vec![],
            podcast_core::NostrVisibility::Public,
            false,
        );
        s.set_nostr_enabled(true);
    }
    let handler = handler_with_store(store.clone());
    create_owned(&handler, id.clone());
    publish_show(&handler, id.clone()); // stamp last_published_at

    let out = delete_owned(&handler, id.clone());
    assert_eq!(out["ok"], true);
    assert_eq!(
        out["deletion_status"].as_str().unwrap_or(""),
        "signed",
        "deletion must be kernel-routed via per-podcast signer (D13): {out}"
    );
    // D13: no raw event id returned — kernel owns signing.
    assert!(
        out.get("deletion_event_id").is_none() || out["deletion_event_id"].is_null(),
        "deletion_event_id must be absent (D13 — kernel-signed, not app-signed): {out}"
    );
}
