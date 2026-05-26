//! NIP-22 (kind 1111) episode-comments handlers.
//!
//! Replaces the stubs that returned `nostr_relay_pending`.
//!
//! ## Wire shape (NIP-22 / NIP-73)
//!
//! kind 1111 events reference the episode via NIP-73 tags:
//! * `["i", "podcast:item:guid:<guid>"]` — the target content identifier
//! * `["k", "podcast:item:guid"]` — the target content kind namespace
//!
//! Matching the original `App/Sources/Services/NostrCommentService.swift`.
//!
//! ## Relay
//!
//! Both fetch and publish dispatch to `relay.primal.net` via the
//! `nostr_relay` capability (wired in `PodcastHostOpHandler`).
//!
//! ## Cache
//!
//! Comments are stored in `PodcastHostOpHandler::comments_cache`
//! (`Arc<Mutex<HashMap<String, Vec<CommentSummary>>>>`) keyed by episode_id.
//! The snapshot builder projects the cache slice for the now-playing episode.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use nostr::{EventBuilder, Keys, Kind, Tag};
use nostr::nips::nip19::ToBech32;

use crate::capability::nostr_relay::{
    NostrRelayRequest, NostrRelayResult, NOSTR_RELAY_CAPABILITY_NAMESPACE,
};
use crate::comments_anchor::episode_nip73_anchor;
use crate::ffi::projections::CommentSummary;
use crate::store::{identity::IdentityStore, PodcastStore};
use nmp_core::substrate::CapabilityRequest;
use nmp_ffi::NmpApp;

/// Default relay for comment operations.
const COMMENT_RELAY: &str = "wss://relay.primal.net";

/// Dispatch a `NostrRelayRequest` via the capability ABI and decode the result.
///
/// Mirrors `PodcastHostOpHandler::dispatch_http` but for the `nostr_relay`
/// namespace. The capability executor drives the real WebSocket.
pub(crate) fn dispatch_nostr_relay(
    app: *mut NmpApp,
    req: &NostrRelayRequest,
    correlation_id: &str,
) -> Result<NostrRelayResult, String> {
    let payload_json = serde_json::to_string(req).map_err(|e| e.to_string())?;
    let cap_req = CapabilityRequest {
        namespace: NOSTR_RELAY_CAPABILITY_NAMESPACE.to_owned(),
        correlation_id: correlation_id.to_owned(),
        payload_json,
    };
    // SAFETY: caller holds the same pointer contract as dispatch_http.
    let envelope = unsafe { &*app }.dispatch_capability(&cap_req);
    serde_json::from_str::<NostrRelayResult>(&envelope.result_json)
        .map_err(|e| format!("decode nostr_relay result: {e}"))
}

/// Fetch kind-1111 comments for `episode_id` from relay.primal.net,
/// parse them into `CommentSummary` rows, and write to the cache.
///
/// Returns `{"ok":true}` on success; `{"ok":false,"error":"..."}` on failure.
pub fn handle_fetch_comments(
    app: *mut NmpApp,
    store: &Arc<Mutex<PodcastStore>>,
    comments_cache: &Arc<Mutex<HashMap<String, Vec<CommentSummary>>>>,
    rev: &Arc<std::sync::atomic::AtomicU64>,
    episode_id: &str,
    correlation_id: &str,
) -> serde_json::Value {
    let anchor = match store.lock() {
        Ok(s) => match episode_nip73_anchor(&s, episode_id) {
            Some(a) => a,
            None => return serde_json::json!({"ok": false, "error": "episode not found"}),
        },
        Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
    };

    let filter = serde_json::json!({
        "kinds": [1111],
        "#i": [anchor],
        "limit": 100
    });

    let relay_req = NostrRelayRequest::Subscribe {
        sub_id: format!("comments-{episode_id}"),
        filter,
        relay_urls: vec![COMMENT_RELAY.into()],
        timeout_ms: 8_000,
    };

    let result = match dispatch_nostr_relay(app, &relay_req, correlation_id) {
        Ok(r) => r,
        Err(e) => return serde_json::json!({"ok": false, "error": e}),
    };

    let events = match result {
        NostrRelayResult::Events { events, .. } => events,
        NostrRelayResult::Error { message } => {
            return serde_json::json!({"ok": false, "error": message});
        }
        NostrRelayResult::Published { .. } => {
            return serde_json::json!({"ok": false, "error": "unexpected Published result"});
        }
    };

    let mut summaries: Vec<CommentSummary> = Vec::with_capacity(events.len());
    for ev in &events {
        let id = ev["id"].as_str().unwrap_or("").to_string();
        let pubkey_hex = ev["pubkey"].as_str().unwrap_or("");
        let content = ev["content"].as_str().unwrap_or("").to_string();
        let created_at = ev["created_at"].as_i64().unwrap_or(0);

        // Encode the pubkey to npub bech32 so iOS renders it without needing
        // a bech32 dependency (same contract as CommentSummary.author_npub).
        let author_npub = nostr::PublicKey::parse(pubkey_hex)
            .ok()
            .and_then(|pk| pk.to_bech32().ok())
            .unwrap_or_else(|| pubkey_hex.to_string());

        if id.is_empty() {
            continue;
        }
        summaries.push(CommentSummary {
            id,
            author_npub,
            author_name: None,
            content,
            created_at,
        });
    }

    // Newest-first so iOS renders the most recent comment at the top.
    summaries.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    match comments_cache.lock() {
        Ok(mut cache) => {
            cache.insert(episode_id.to_string(), summaries);
        }
        Err(_) => return serde_json::json!({"ok": false, "error": "comments_cache poisoned"}),
    }

    rev.fetch_add(1, Ordering::Relaxed);
    serde_json::json!({"ok": true})
}

/// Sign and publish a kind-1111 comment for `episode_id` to relay.primal.net.
/// Optimistically prepends the new comment to the local cache on success.
///
/// Returns `{"ok":false,"error":"not signed in"}` when no identity is loaded.
pub fn handle_post_comment(
    app: *mut NmpApp,
    store: &Arc<Mutex<PodcastStore>>,
    identity: &Arc<Mutex<IdentityStore>>,
    comments_cache: &Arc<Mutex<HashMap<String, Vec<CommentSummary>>>>,
    rev: &Arc<std::sync::atomic::AtomicU64>,
    episode_id: &str,
    content: &str,
    correlation_id: &str,
) -> serde_json::Value {
    if content.trim().is_empty() {
        return serde_json::json!({"ok": false, "error": "empty comment"});
    }

    // Resolve identity.
    let secret_hex = match identity.lock() {
        Ok(id) => match id.secret_hex.clone() {
            Some(s) => s,
            None => return serde_json::json!({"ok": false, "error": "not signed in"}),
        },
        Err(_) => return serde_json::json!({"ok": false, "error": "identity poisoned"}),
    };

    // Resolve anchor.
    let anchor = match store.lock() {
        Ok(s) => match episode_nip73_anchor(&s, episode_id) {
            Some(a) => a,
            None => return serde_json::json!({"ok": false, "error": "episode not found"}),
        },
        Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
    };

    // Build and sign the kind-1111 event.
    let keys = match Keys::parse(&secret_hex) {
        Ok(k) => k,
        Err(e) => return serde_json::json!({"ok": false, "error": format!("key parse: {e}")}),
    };

    let tags = vec![
        Tag::parse(["i", &anchor]).expect("static tag i anchor"),
        Tag::parse(["k", "podcast:item:guid"]).expect("static tag k anchor"),
    ];

    let event = match EventBuilder::new(Kind::Comment, content)
        .tags(tags)
        .sign_with_keys(&keys)
    {
        Ok(ev) => ev,
        Err(e) => return serde_json::json!({"ok": false, "error": format!("sign: {e}")}),
    };

    let event_id = event.id.to_hex();
    let author_npub = keys
        .public_key()
        .to_bech32()
        .unwrap_or_else(|_| keys.public_key().to_hex());

    let event_json = match serde_json::to_string(&event) {
        Ok(j) => j,
        Err(e) => return serde_json::json!({"ok": false, "error": format!("serialize: {e}")}),
    };

    // Publish.
    let relay_req = NostrRelayRequest::Publish {
        event_json: event_json.clone(),
        relay_urls: vec![COMMENT_RELAY.into()],
    };

    let result = match dispatch_nostr_relay(app, &relay_req, correlation_id) {
        Ok(r) => r,
        Err(e) => return serde_json::json!({"ok": false, "error": e}),
    };

    let ok = match &result {
        NostrRelayResult::Published { ok, .. } => *ok,
        NostrRelayResult::Error { message } => {
            return serde_json::json!({"ok": false, "error": message});
        }
        NostrRelayResult::Events { .. } => false,
    };

    if !ok {
        let errors = match &result {
            NostrRelayResult::Published { errors, .. } => errors
                .iter()
                .map(|(_, m)| m.as_str())
                .collect::<Vec<_>>()
                .join("; "),
            _ => "unknown publish error".into(),
        };
        return serde_json::json!({"ok": false, "error": errors});
    }

    // Optimistic cache update — prepend so the new comment is at the top.
    let new_comment = CommentSummary {
        id: event_id.clone(),
        author_npub,
        author_name: None,
        content: content.to_string(),
        created_at: event.created_at.as_secs() as i64,
    };

    if let Ok(mut cache) = comments_cache.lock() {
        cache
            .entry(episode_id.to_string())
            .or_default()
            .insert(0, new_comment);
    }

    rev.fetch_add(1, Ordering::Relaxed);
    serde_json::json!({"ok": true, "event_id": event_id})
}

#[cfg(test)]
#[path = "comments_handler_tests.rs"]
mod tests;
