//! NIP-F4 publish handlers — actor-thread implementation for the
//! `podcast.publish.*` action namespace (features #27/#28).
//!
//! Each function builds a NIP-F4 event (kind:10154 show, kind:54
//! episode, kind:10064 author-claim) from the podcast-domain types in
//! [`crate::store::PodcastStore`] + the per-podcast keypair stored in
//! [`crate::store::PodcastKeyStore`]. The constructed event is
//! returned to the caller via the `{"ok":true,"status":"relay_pending",
//! "event_tags":[...]}` envelope and (for `publish_show`) stamped onto
//! [`OwnedPublishState`] so the FFI snapshot can render "last published".
//!
//! The relay-side broadcast itself is **out of scope** for this PR —
//! the broader NMP per-podcast-key signing infrastructure isn't wired
//! yet. The returned `status: "relay_pending"` tells the iOS shell to
//! render "queued; awaiting relay" until that layer lands.
//!
//! Lives in a sibling module to keep [`crate::host_op_handler`] under
//! the 500-LOC hard limit (AGENTS.md).

use std::sync::atomic::Ordering;

use chrono::Utc;
use podcast_discovery::{
    episode_to_episode_tags, podcast_to_show_tags, show_content, KIND_AUTHOR_CLAIM, KIND_EPISODE,
    KIND_SHOW,
};

use crate::ffi::actions::publish_module::PublishAction;
use crate::ffi::handle::OwnedPublishState;
use crate::host_op_handler::PodcastHostOpHandler;

/// Dispatch entry-point — match the typed enum variant to the per-op
/// handler. The caller (the `HostOpHandler::handle` impl in
/// `host_op_handler.rs`) deserializes the `PublishAction` first; this
/// module is pure routing once that decode succeeds.
pub fn handle_publish_action(
    handler: &PodcastHostOpHandler,
    action: PublishAction,
) -> serde_json::Value {
    match action {
        PublishAction::CreateOwnedPodcast { podcast_id } => create_owned(handler, podcast_id),
        PublishAction::PublishShow { podcast_id } => publish_show(handler, podcast_id),
        PublishAction::PublishEpisode { episode_id } => publish_episode(handler, episode_id),
        PublishAction::PublishAuthorClaim { agent_pubkey_hex } => {
            publish_author_claim(handler, agent_pubkey_hex)
        }
        PublishAction::RemoveOwnedPodcast { podcast_id } => remove_owned(handler, podcast_id),
    }
}

/// `podcast.publish.create_owned_podcast` — generate a per-podcast
/// keypair, stamp `owner_pubkey_hex` onto the podcast row, and bump
/// `rev` so the iOS snapshot poll picks it up.
fn create_owned(handler: &PodcastHostOpHandler, podcast_id: String) -> serde_json::Value {
    let exists = match handler.store.lock() {
        Ok(s) => s.podcast_by_id_str(&podcast_id).is_some(),
        Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
    };
    if !exists {
        return serde_json::json!({
            "ok": false,
            "error": format!("podcast not found: {podcast_id}")
        });
    }
    let pubkey_hex = match handler.podcast_keys.lock() {
        Ok(mut keys) => {
            keys.generate_key(&podcast_id);
            let pk = match keys.pubkey_hex(&podcast_id) {
                Some(pk) => pk,
                None => return serde_json::json!({"ok": false, "error": "derive pubkey failed"}),
            };
            keys.save_to_disk();
            pk
        }
        Err(_) => return serde_json::json!({"ok": false, "error": "podcast_keys poisoned"}),
    };
    if let Ok(mut s) = handler.store.lock() {
        s.set_owner_pubkey_hex(&podcast_id, pubkey_hex.clone());
    }
    if let Ok(mut state) = handler.publish_state.lock() {
        let _: &mut OwnedPublishState = state.entry(podcast_id).or_default();
    }
    handler.rev.fetch_add(1, Ordering::Relaxed);
    serde_json::json!({"ok": true, "pubkey_hex": pubkey_hex})
}

/// `podcast.publish.publish_show` — build a `kind:10154` show event
/// from the podcast row + its per-podcast key. The unsigned event
/// JSON is stamped onto `publish_state[podcast_id].show_event_json`.
fn publish_show(handler: &PodcastHostOpHandler, podcast_id: String) -> serde_json::Value {
    let podcast_clone = match handler.store.lock() {
        Ok(s) => match s.podcast_by_id_str(&podcast_id) {
            Some(p) => p.clone(),
            None => return serde_json::json!({
                "ok": false,
                "error": format!("podcast not found: {podcast_id}")
            }),
        },
        Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
    };
    let pubkey_hex = match handler.podcast_keys.lock() {
        Ok(keys) => match keys.pubkey_hex(&podcast_id) {
            Some(pk) => pk,
            None => return serde_json::json!({
                "ok": false,
                "error": "podcast not owned (run create_owned_podcast first)"
            }),
        },
        Err(_) => return serde_json::json!({"ok": false, "error": "podcast_keys poisoned"}),
    };

    let tags = podcast_to_show_tags(&podcast_clone, &pubkey_hex);
    let content = show_content(&podcast_clone);
    let created_at = Utc::now().timestamp();
    let event_json = build_unsigned_event_json(KIND_SHOW, &pubkey_hex, created_at, &tags, &content);

    if let Ok(mut state) = handler.publish_state.lock() {
        let entry: &mut OwnedPublishState = state.entry(podcast_id).or_default();
        entry.show_event_json = Some(event_json.clone());
        entry.last_published_at = Some(created_at);
    }
    handler.rev.fetch_add(1, Ordering::Relaxed);
    serde_json::json!({
        "ok": true,
        "status": "relay_pending",
        "event_tags": tags,
        "event_json": event_json,
    })
}

/// `podcast.publish.publish_episode` — build a `kind:54` episode event
/// from the episode row + its podcast's per-podcast key. The parent
/// podcast must have been claimed via `create_owned_podcast`.
fn publish_episode(handler: &PodcastHostOpHandler, episode_id: String) -> serde_json::Value {
    let (podcast, episode) = match handler.store.lock() {
        Ok(s) => match s.episode_with_podcast_clone(&episode_id) {
            Some(pair) => pair,
            None => return serde_json::json!({
                "ok": false,
                "error": format!("episode not found: {episode_id}")
            }),
        },
        Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
    };
    let podcast_id_str = podcast.id.0.to_string();
    let pubkey_hex = match handler.podcast_keys.lock() {
        Ok(keys) => match keys.pubkey_hex(&podcast_id_str) {
            Some(pk) => pk,
            None => return serde_json::json!({
                "ok": false,
                "error": "podcast not owned (run create_owned_podcast first)"
            }),
        },
        Err(_) => return serde_json::json!({"ok": false, "error": "podcast_keys poisoned"}),
    };

    let tags = episode_to_episode_tags(&episode);
    let content = episode.description.clone();
    let created_at = Utc::now().timestamp();
    let event_json =
        build_unsigned_event_json(KIND_EPISODE, &pubkey_hex, created_at, &tags, &content);

    handler.rev.fetch_add(1, Ordering::Relaxed);
    serde_json::json!({
        "ok": true,
        "status": "relay_pending",
        "event_tags": tags,
        "event_json": event_json,
    })
}

/// `podcast.publish.publish_author_claim` — build a `kind:10064`
/// author-claim event listing one `["p", podcast_pubkey_hex]` per
/// owned podcast under the supplied agent pubkey.
fn publish_author_claim(
    handler: &PodcastHostOpHandler,
    agent_pubkey_hex: String,
) -> serde_json::Value {
    if agent_pubkey_hex.is_empty() {
        return serde_json::json!({"ok": false, "error": "agent_pubkey_hex is empty"});
    }
    let pairs = match handler.podcast_keys.lock() {
        Ok(keys) => keys.iter_pubkeys(),
        Err(_) => return serde_json::json!({"ok": false, "error": "podcast_keys poisoned"}),
    };
    let mut tags: Vec<Vec<String>> = Vec::with_capacity(pairs.len());
    for (_, pk) in &pairs {
        tags.push(vec!["p".into(), pk.clone()]);
    }
    let created_at = Utc::now().timestamp();
    let event_json =
        build_unsigned_event_json(KIND_AUTHOR_CLAIM, &agent_pubkey_hex, created_at, &tags, "");
    handler.rev.fetch_add(1, Ordering::Relaxed);
    serde_json::json!({
        "ok": true,
        "status": "relay_pending",
        "event_tags": tags,
        "event_json": event_json,
        "owned_count": pairs.len(),
    })
}

/// `podcast.publish.remove_owned_podcast` — drop the per-podcast key,
/// clear `owner_pubkey_hex` from the podcast row, and discard the
/// publish state for that podcast.
fn remove_owned(handler: &PodcastHostOpHandler, podcast_id: String) -> serde_json::Value {
    if let Ok(mut keys) = handler.podcast_keys.lock() {
        keys.remove_key(&podcast_id);
        keys.save_to_disk();
    }
    if let Ok(mut s) = handler.store.lock() {
        s.clear_owner_pubkey_hex(&podcast_id);
    }
    if let Ok(mut state) = handler.publish_state.lock() {
        state.remove(&podcast_id);
    }
    handler.rev.fetch_add(1, Ordering::Relaxed);
    serde_json::json!({"ok": true})
}

/// Build an unsigned Nostr event JSON for diagnostic surfacing on the
/// iOS snapshot. The relay path (sig, id, broadcast) lands when
/// per-podcast keys are wired through the NMP signing pipeline.
fn build_unsigned_event_json(
    kind: u32,
    pubkey_hex: &str,
    created_at: i64,
    tags: &[Vec<String>],
    content: &str,
) -> String {
    let value = serde_json::json!({
        "kind": kind,
        "pubkey": pubkey_hex,
        "created_at": created_at,
        "tags": tags,
        "content": content,
        "id": null,
        "sig": null,
    });
    serde_json::to_string(&value).unwrap_or_else(|_| "{}".into())
}

#[cfg(test)]
#[path = "host_op_publish_tests.rs"]
mod tests;
