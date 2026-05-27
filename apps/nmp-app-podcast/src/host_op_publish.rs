//! NIP-F4 publish handlers — actor-thread implementation for the
//! `podcast.publish.*` action namespace (features #27/#28).
//!
//! Each function builds a signed NIP-F4 event (kind:10154 show, kind:54
//! episode, kind:10064 author-claim) using real secp256k1 cryptography via
//! the `nostr` crate, then broadcasts it to `relay.primal.net` through the
//! `nostr_relay` capability.
//!
//! Return envelope:
//!   - `status: "published"` — event signed AND relay accepted it.
//!   - `status: "signed"` — event signed, but relay dispatch was skipped
//!     (null app pointer in unit tests) or the relay returned an error.
//!   - `status: "relay_pending"` is no longer used; removed in PR 8.
//!
//! Lives in a sibling module to keep [`crate::host_op_handler`] under
//! the 500-LOC hard limit (AGENTS.md).

use std::sync::atomic::Ordering;

use chrono::Utc;
use nostr::{EventBuilder, JsonUtil, Keys, Kind, SecretKey, Tag, Timestamp};
use podcast_discovery::{
    episode_to_episode_tags, podcast_to_show_tags, show_content, KIND_AUTHOR_CLAIM, KIND_EPISODE,
    KIND_SHOW,
};

use crate::capability::{NostrRelayRequest, NostrRelayResult, NOSTR_RELAY_CAPABILITY_NAMESPACE};
use crate::ffi::actions::publish_module::PublishAction;
use crate::ffi::handle::OwnedPublishState;
use crate::host_op_handler::PodcastHostOpHandler;
use nmp_core::substrate::CapabilityRequest;

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

/// `podcast.publish.publish_show` — build and sign a `kind:10154` show
/// event, then broadcast it to `relay.primal.net` via the `nostr_relay`
/// capability. The signed event JSON is stamped onto
/// `publish_state[podcast_id].show_event_json`.
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
    let (pubkey_hex, secret_bytes) = match handler.podcast_keys.lock() {
        Ok(keys) => {
            let pk = match keys.pubkey_hex(&podcast_id) {
                Some(pk) => pk,
                None => return serde_json::json!({
                    "ok": false,
                    "error": "podcast not owned (run create_owned_podcast first)"
                }),
            };
            let sk = match keys.get_key(&podcast_id) {
                Some(b) => *b,
                None => return serde_json::json!({
                    "ok": false,
                    "error": "key vanished between pubkey_hex and get_key"
                }),
            };
            (pk, sk)
        }
        Err(_) => return serde_json::json!({"ok": false, "error": "podcast_keys poisoned"}),
    };

    let tags = podcast_to_show_tags(&podcast_clone, &pubkey_hex);
    let content = show_content(&podcast_clone);
    let created_at = Utc::now().timestamp();

    let (event_json, event_id) = match sign_event(&secret_bytes, KIND_SHOW, &tags, &content, created_at) {
        Ok(pair) => pair,
        Err(e) => return serde_json::json!({"ok": false, "error": format!("signing failed: {e}")}),
    };

    if let Ok(mut state) = handler.publish_state.lock() {
        let entry: &mut OwnedPublishState = state.entry(podcast_id).or_default();
        entry.show_event_json = Some(event_json.clone());
        entry.last_published_at = Some(created_at);
    }
    handler.rev.fetch_add(1, Ordering::Relaxed);

    let status = dispatch_nostr_relay(handler, &event_json);
    serde_json::json!({
        "ok": true,
        "status": status,
        "event_id": event_id,
        "event_tags": tags,
        "event_json": event_json,
    })
}

/// `podcast.publish.publish_episode` — build and sign a `kind:54` episode
/// event, then broadcast to `relay.primal.net`. The parent podcast must
/// have been claimed via `create_owned_podcast`.
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
    let (pubkey_hex, secret_bytes) = match handler.podcast_keys.lock() {
        Ok(keys) => {
            let pk = match keys.pubkey_hex(&podcast_id_str) {
                Some(pk) => pk,
                None => return serde_json::json!({
                    "ok": false,
                    "error": "podcast not owned (run create_owned_podcast first)"
                }),
            };
            let sk = match keys.get_key(&podcast_id_str) {
                Some(b) => *b,
                None => return serde_json::json!({
                    "ok": false,
                    "error": "key vanished between pubkey_hex and get_key"
                }),
            };
            (pk, sk)
        }
        Err(_) => return serde_json::json!({"ok": false, "error": "podcast_keys poisoned"}),
    };
    let _ = pubkey_hex; // pubkey is embedded in the signed event; not needed directly

    let tags = episode_to_episode_tags(&episode);
    let content = episode.description.clone();
    let created_at = Utc::now().timestamp();

    let (event_json, event_id) = match sign_event(&secret_bytes, KIND_EPISODE, &tags, &content, created_at) {
        Ok(pair) => pair,
        Err(e) => return serde_json::json!({"ok": false, "error": format!("signing failed: {e}")}),
    };

    handler.rev.fetch_add(1, Ordering::Relaxed);
    let status = dispatch_nostr_relay(handler, &event_json);
    serde_json::json!({
        "ok": true,
        "status": status,
        "event_id": event_id,
        "event_tags": tags,
        "event_json": event_json,
    })
}

/// `podcast.publish.publish_author_claim` — build and sign a `kind:10064`
/// author-claim event listing one `["p", podcast_pubkey_hex]` per owned
/// podcast, signed with the active agent identity from
/// `NmpApp::active_local_keys()`. When no agent keys are available (unit
/// tests, or before login), the event JSON is returned unsigned and
/// `status: "signed"` is used so callers know relay dispatch was skipped.
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

    // Attempt to sign with the active agent identity (NmpApp::active_local_keys).
    // Falls back to an unsigned placeholder when the app pointer is null or no
    // keys are loaded (unit-test and pre-login scenarios).
    let agent_keys: Option<nostr::Keys> = if handler.app.is_null() {
        None
    } else {
        // SAFETY: app is non-null and caller guarantees the pointer is live for
        // the duration of this call (same invariant as dispatch_nostr_relay).
        let slot = unsafe { &*handler.app }.active_local_keys();
        slot.lock().ok().and_then(|guard| guard.clone())
    };

    match agent_keys {
        Some(keys) => {
            let secret_bytes = keys.secret_key().to_secret_bytes();
            match sign_event(&secret_bytes, KIND_AUTHOR_CLAIM, &tags, "", created_at) {
                Ok((event_json, event_id)) => {
                    handler.rev.fetch_add(1, Ordering::Relaxed);
                    let status = dispatch_nostr_relay(handler, &event_json);
                    serde_json::json!({
                        "ok": true,
                        "status": status,
                        "event_id": event_id,
                        "event_tags": tags,
                        "event_json": event_json,
                        "owned_count": pairs.len(),
                    })
                }
                Err(e) => serde_json::json!({"ok": false, "error": format!("signing failed: {e}")}),
            }
        }
        None => {
            // No agent keys — return the tag list but mark as unsigned so the
            // caller knows it was not broadcast.
            let unsigned = serde_json::json!({
                "kind": KIND_AUTHOR_CLAIM,
                "pubkey": agent_pubkey_hex,
                "created_at": created_at,
                "tags": tags,
                "content": "",
                "id": null,
                "sig": null,
            });
            handler.rev.fetch_add(1, Ordering::Relaxed);
            serde_json::json!({
                "ok": true,
                "status": "signed",
                "event_tags": tags,
                "event_json": unsigned.to_string(),
                "owned_count": pairs.len(),
            })
        }
    }
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

/// Sign a Nostr event with the given secret key. Returns `(event_json,
/// event_id_hex)` on success. Tags that fail `Tag::parse` are silently
/// dropped (malformed input is logged to stderr).
///
/// `kind_num` is the raw NIP kind integer (e.g. 10154, 54, 10064).
fn sign_event(
    secret_bytes: &[u8; 32],
    kind_num: u32,
    tags: &[Vec<String>],
    content: &str,
    created_at_secs: i64,
) -> Result<(String, String), String> {
    let sk = SecretKey::from_slice(secret_bytes)
        .map_err(|e| format!("invalid secret key: {e}"))?;
    let keys = Keys::new(sk);

    let nostr_tags: Vec<Tag> = tags
        .iter()
        .filter_map(|t| {
            match Tag::parse(t) {
                Ok(tag) => Some(tag),
                Err(e) => {
                    eprintln!("[host_op_publish] dropping malformed tag {:?}: {e}", t);
                    None
                }
            }
        })
        .collect();

    let kind = Kind::from(kind_num as u16);
    let ts = Timestamp::from(created_at_secs as u64);
    let event = EventBuilder::new(kind, content)
        .tags(nostr_tags)
        .custom_created_at(ts)
        .sign_with_keys(&keys)
        .map_err(|e| format!("sign error: {e}"))?;

    let event_id = event.id.to_hex();
    let event_json = event.as_json();
    Ok((event_json, event_id))
}

/// Dispatch a signed event JSON string to `relay.primal.net` via the
/// `nostr_relay` capability. Returns `"published"` if the relay accepted
/// the event, `"signed"` otherwise (null app pointer, parse error, or
/// relay rejection).
///
/// Null-app guard: unit tests run with `app == null_mut()`. Dispatching
/// a capability through a null pointer is UB — we return `"signed"` early.
fn dispatch_nostr_relay(handler: &PodcastHostOpHandler, event_json: &str) -> &'static str {
    if handler.app.is_null() {
        return "signed";
    }

    let relay_req = NostrRelayRequest::Publish {
        event_json: event_json.to_owned(),
        relay_urls: vec!["wss://relay.primal.net".to_owned()],
    };
    let payload_json = match serde_json::to_string(&relay_req) {
        Ok(j) => j,
        Err(_) => return "signed",
    };
    let cap_req = CapabilityRequest {
        namespace: NOSTR_RELAY_CAPABILITY_NAMESPACE.to_owned(),
        correlation_id: uuid::Uuid::new_v4().to_string(),
        payload_json,
    };

    // SAFETY: app is non-null (checked above). The pointer is valid for the
    // duration of this call — same invariant as `dispatch_http` in
    // `host_op_handler.rs`.
    let envelope = unsafe { &*handler.app }.dispatch_capability(&cap_req);
    match serde_json::from_str::<NostrRelayResult>(&envelope.result_json) {
        Ok(NostrRelayResult::Published { ok: true, .. }) => "published",
        _ => "signed",
    }
}

#[cfg(test)]
#[path = "host_op_publish_tests.rs"]
mod tests;
