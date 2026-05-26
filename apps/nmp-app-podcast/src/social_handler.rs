//! Handler for the `podcast.fetch_contacts` action — wires real kind:3
//! contact-list subscription and kind:0 profile hydration via relay.primal.net.
//!
//! ## Flow
//!
//! 1. Read the active `pubkey_hex` from `IdentityStore`.
//! 2. Subscribe to `{"kinds":[3],"authors":[pubkey_hex],"limit":1}` —
//!    grabs the user's NIP-02 follow list.
//! 3. Parse the `p` tags to extract follow pubkeys.
//! 4. Batch-fetch `{"kinds":[0],"authors":[...up to 50 pubkeys]}` to hydrate
//!    NIP-01 profile metadata (display_name, picture, name).
//! 5. Build a `SocialSnapshot` with `ContactSummary` rows (npub bech32-encoded).
//! 6. Store the snapshot in the `social` slot and bump `rev`.

use std::sync::atomic::Ordering;
use std::time::Duration;

use nostr::nips::nip19::ToBech32;
use serde_json::json;

use crate::ffi::projections::{ContactSummary, SocialSnapshot};
use crate::host_op_handler::PodcastHostOpHandler;

/// Default relay used for social-graph fetches.
const RELAY_URL: &str = "wss://relay.primal.net";

/// Fetch the active user's NIP-02 follow list and hydrate kind:0 metadata
/// for each follow. Stores the resulting [`SocialSnapshot`] in
/// `handler.social` and increments `handler.rev`.
///
/// Returns `{"ok":true,"following_count":N}` on success, or
/// `{"ok":false,"error":"..."}` on any hard failure.
pub fn handle_fetch_contacts(handler: &PodcastHostOpHandler) -> serde_json::Value {
    // 1. Get the active pubkey.
    let pubkey_hex = match handler.identity.lock() {
        Ok(id) => match id.pubkey_hex.clone() {
            Some(pk) => pk,
            None => return json!({"ok": false, "error": "not signed in"}),
        },
        Err(_) => return json!({"ok": false, "error": "identity lock poisoned"}),
    };

    let relay_urls = vec![RELAY_URL.to_string()];

    // 2. Fetch kind:3 (contact list) for the active user.
    let kind3_events = fetch_relay_events(
        &handler.runtime,
        json!({"kinds": [3], "authors": [&pubkey_hex], "limit": 1}),
        &relay_urls,
        8_000,
    );

    // 3. Parse follow pubkeys from `p` tags.
    let follow_pubkeys: Vec<String> = kind3_events
        .iter()
        .flat_map(|ev| {
            ev["tags"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|t| {
                    let arr = t.as_array()?;
                    if arr.first()?.as_str()? == "p" {
                        arr.get(1)?.as_str().map(str::to_string)
                    } else {
                        None
                    }
                })
        })
        .collect();

    let following_count = follow_pubkeys.len();

    // 4. Batch-fetch kind:0 metadata for follows (cap at 50 for speed).
    let batch: Vec<String> = follow_pubkeys.iter().take(50).cloned().collect();
    let kind0_events = if !batch.is_empty() {
        fetch_relay_events(
            &handler.runtime,
            json!({"kinds": [0], "authors": batch}),
            &relay_urls,
            8_000,
        )
    } else {
        vec![]
    };

    // 5. Build ContactSummary list with bech32-encoded npub.
    let contacts: Vec<ContactSummary> = follow_pubkeys
        .iter()
        .map(|pk| {
            let npub = nostr::PublicKey::parse(pk)
                .ok()
                .and_then(|pub_key| pub_key.to_bech32().ok())
                .unwrap_or_else(|| pk.clone());

            let meta = kind0_events
                .iter()
                .find(|ev| ev["pubkey"].as_str() == Some(pk.as_str()));

            let profile = meta.and_then(|ev| {
                serde_json::from_str::<serde_json::Value>(
                    ev["content"].as_str().unwrap_or("{}"),
                )
                .ok()
            });

            let display_name = profile
                .as_ref()
                .and_then(|p| {
                    p["display_name"]
                        .as_str()
                        .or_else(|| p["name"].as_str())
                })
                .map(str::to_string);

            let picture_url = profile
                .as_ref()
                .and_then(|p| p["picture"].as_str())
                .map(str::to_string);

            ContactSummary {
                npub,
                display_name,
                picture_url,
            }
        })
        .collect();

    // 6. Store the snapshot and bump rev.
    if let Ok(mut social) = handler.social.lock() {
        *social = Some(SocialSnapshot {
            following: contacts,
            following_count,
        });
    }
    handler.rev.fetch_add(1, Ordering::Relaxed);

    json!({"ok": true, "following_count": following_count})
}

/// Block the caller and run an async subscribe-until-EOSE on the given filter.
fn fetch_relay_events(
    runtime: &tokio::runtime::Runtime,
    filter: serde_json::Value,
    relay_urls: &[String],
    timeout_ms: u64,
) -> Vec<serde_json::Value> {
    let sub_id = uuid::Uuid::new_v4().to_string();
    let timeout_dur = Duration::from_millis(timeout_ms);
    let relay_urls = relay_urls.to_vec();
    runtime.block_on(async move {
        crate::relay::subscribe_until_eose(&sub_id, &filter, &relay_urls, timeout_dur).await
    })
}

#[cfg(test)]
#[path = "social_handler_tests.rs"]
mod tests;
