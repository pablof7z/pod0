//! NIP-F4 publish handlers — actor-thread implementation for the
//! `podcast.publish.*` action namespace (features #27/#28).
//!
//! Per-podcast events (kind:10154/54) are signed by the NMP kernel using the
//! per-podcast NIP-F4 key registered as a non-active signer via
//! `nmp_app_signin_nsec(make_active=0)`. The kernel's `sign_with_account_nonblocking`
//! routes to the named pubkey across local-key + remote maps — no raw secret
//! bytes in app code (D13). The author claim (kind:10064) uses
//! `nmp.publish { PublishRaw }` signed by the active user signer, same as before.
//!
//! Return envelope:
//!   - `status: "queued"` — handed to NMP for relay routing (async).
//!   - `status: "signed"` — null app pointer in unit tests.
//!
//! Lives in a sibling module to keep [`crate::host_op_handler`] under
//! the 500-LOC hard limit (AGENTS.md).

use std::sync::atomic::Ordering;

use podcast_discovery::{
    episode_to_episode_tags, podcast_to_show_tags, show_content, KIND_AUTHOR_CLAIM, KIND_EPISODE,
    KIND_SHOW,
};
use podcast_core::NostrVisibility;

use crate::ffi::actions::publish_module::PublishAction;
use crate::ffi::handle::OwnedPublishState;
use crate::host_op_handler::PodcastHostOpHandler;
use crate::nmp_dispatch::{
    blossom_upload_via_nmp, publish_raw_via_nmp, publish_raw_with_signer_via_nmp,
    register_podcast_signer_in_kernel,
};
use crate::store::podcast_keys::secret_to_hex;

/// Dispatch entry-point — match the typed enum variant to the per-op
/// handler. The caller (the `HostOpHandler::handle` impl in
/// `host_op_handler.rs`) deserializes the `PublishAction` first; this
/// module is pure routing once that decode succeeds.
pub fn handle_publish_action(
    handler: &PodcastHostOpHandler,
    action: PublishAction,
) -> serde_json::Value {
    match action {
        // Update / delete lifecycle lives in the sibling module (keeps this
        // file under the 500-LOC hard limit). It owns its own variant
        // destructuring via `handle_lifecycle_action`.
        action @ (PublishAction::UpdateOwnedPodcast { .. }
        | PublishAction::DeleteOwnedPodcast { .. }) => {
            crate::host_op_publish_lifecycle::handle_lifecycle_action(handler, action)
        }
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
/// `rev` so the next iOS snapshot frame picks it up.
pub(crate) fn create_owned(
    handler: &PodcastHostOpHandler,
    podcast_id: String,
) -> serde_json::Value {
    let exists = match handler.state.library.store.lock() {
        Ok(s) => s.podcast_by_id_str(&podcast_id).is_some(),
        Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
    };
    if !exists {
        return serde_json::json!({
            "ok": false,
            "error": format!("podcast not found: {podcast_id}")
        });
    }
    let pubkey_hex = match handler.state.publish.podcast_keys.lock() {
        Ok(mut keys) => {
            keys.generate_key(&podcast_id);
            let pk = match keys.pubkey_hex(&podcast_id) {
                Some(pk) => pk,
                None => return serde_json::json!({"ok": false, "error": "derive pubkey failed"}),
            };
            pk
        }
        Err(_) => return serde_json::json!({"ok": false, "error": "podcast_keys poisoned"}),
    };
    if let Ok(mut s) = handler.state.library.store.lock() {
        s.set_owner_pubkey_hex(&podcast_id, pubkey_hex.clone());
    }
    if let Ok(mut state) = handler.state.publish.publish_state.lock() {
        let _: &mut OwnedPublishState = state.entry(podcast_id).or_default();
    }
    handler.state.infra.rev.fetch_add(1, Ordering::Relaxed);
    serde_json::json!({"ok": true, "pubkey_hex": pubkey_hex})
}

/// `podcast.publish.publish_show` — build a `kind:10154` show event and route
/// it through the NMP kernel for signing with the per-podcast NIP-F4 key.
///
/// The key is registered in the kernel's identity roster as a non-active signer
/// (`nmp_app_signin_nsec(make_active=0)`) immediately before the
/// `PublishRaw { signer_pubkey: Some(pubkey_hex) }` dispatch. The FIFO actor
/// queue guarantees the signer is present when the sign-time lookup runs.
/// No secret bytes cross the signing boundary — the kernel holds and uses them.
pub(crate) fn publish_show(
    handler: &PodcastHostOpHandler,
    podcast_id: String,
) -> serde_json::Value {
    let podcast_clone = match handler.state.library.store.lock() {
        Ok(s) => match s.podcast_by_id_str(&podcast_id) {
            Some(p) => p.clone(),
            None => {
                return serde_json::json!({
                    "ok": false,
                    "error": format!("podcast not found: {podcast_id}")
                })
            }
        },
        Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
    };
    let (pubkey_hex, secret_hex) = match handler.state.publish.podcast_keys.lock() {
        Ok(keys) => {
            let pk = match keys.pubkey_hex(&podcast_id) {
                Some(pk) => pk,
                None => {
                    return serde_json::json!({
                        "ok": false,
                        "error": "podcast not owned (run create_owned_podcast first)"
                    })
                }
            };
            let sk = match keys.get_key(&podcast_id) {
                Some(b) => secret_to_hex(b),
                None => {
                    return serde_json::json!({
                        "ok": false,
                        "error": "key vanished between pubkey_hex and get_key"
                    })
                }
            };
            (pk, sk)
        }
        Err(_) => return serde_json::json!({"ok": false, "error": "podcast_keys poisoned"}),
    };

    let tags = podcast_to_show_tags(&podcast_clone, &pubkey_hex);
    let content = show_content(&podcast_clone);

    // Register the per-podcast key in the kernel's identity roster
    // (non-active; FIFO — must arrive before the PublishRaw dispatch).
    register_podcast_signer_in_kernel(handler.app, &secret_hex);

    // Stamp last_published_at before the async dispatch so the UI can show
    // the updated timestamp on the next snapshot even before relay confirm.
    let created_at = chrono::Utc::now().timestamp();
    if let Ok(mut state) = handler.state.publish.publish_state.lock() {
        let entry: &mut OwnedPublishState = state.entry(podcast_id.clone()).or_default();
        entry.last_published_at = Some(created_at);
    }
    handler.state.infra.rev.fetch_add(1, Ordering::Relaxed);

    // Dispatch PublishRaw signed by the per-podcast key (no app-side signing).
    let status =
        publish_raw_with_signer_via_nmp(handler.app, KIND_SHOW, &tags, &content, &pubkey_hex);
    serde_json::json!({
        "ok": true,
        "status": status,
        "pubkey_hex": pubkey_hex,
        "event_tags": tags,
    })
}

/// `podcast.publish.publish_episode` — build a `kind:54` episode event and
/// route it through the NMP kernel for signing with the per-podcast NIP-F4 key.
///
/// If the episode has a local download the kernel's `nmp.blossom.upload` action
/// is dispatched (with the per-podcast key as the kind:24242 signer) and the
/// audio URL is resolved to the Blossom blob once the upload settles
/// asynchronously. On any failure (no local file, null app, upload dispatch
/// rejected) the publish falls back to the RSS enclosure URL.
///
/// Reached either directly via the `podcast.publish` action dispatch, or as
/// the self-enqueued per-episode backfill the lifecycle handler fans out on a
/// private→public flip (see [`crate::host_op_publish_lifecycle::update_owned`]).
fn publish_episode(handler: &PodcastHostOpHandler, episode_id: String) -> serde_json::Value {
    let (podcast, episode, local_path, blossom_servers, nostr_enabled) = match handler.state.library.store.lock() {
        Ok(s) => match s.episode_with_podcast_clone(&episode_id) {
            Some((podcast, episode)) => {
                let local_path = s.local_path_for(&episode.id).map(str::to_owned);
                // Wrap the single blossom_server_url in a vec for the kernel action.
                let server = s.blossom_server_url().to_owned();
                let servers = if server.is_empty() { vec![] } else { vec![server] };
                (podcast, episode, local_path, servers, s.nostr_enabled())
            }
            None => {
                return serde_json::json!({
                    "ok": false,
                    "error": format!("episode not found: {episode_id}")
                })
            }
        },
        Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
    };
    if podcast.nostr_visibility != NostrVisibility::Public {
        return serde_json::json!({
            "ok": false,
            "error": "podcast visibility is not public"
        });
    }
    if !nostr_enabled {
        return serde_json::json!({
            "ok": false,
            "error": "nostr publishing is disabled"
        });
    }
    let podcast_id_str = podcast.id.0.to_string();
    let (pubkey_hex, secret_hex) = match handler.state.publish.podcast_keys.lock() {
        Ok(keys) => {
            let pk = match keys.pubkey_hex(&podcast_id_str) {
                Some(pk) => pk,
                None => {
                    return serde_json::json!({
                        "ok": false,
                        "error": "podcast not owned (run create_owned_podcast first)"
                    })
                }
            };
            let sk = match keys.get_key(&podcast_id_str) {
                Some(b) => secret_to_hex(b),
                None => {
                    return serde_json::json!({
                        "ok": false,
                        "error": "key vanished between pubkey_hex and get_key"
                    })
                }
            };
            (pk, sk)
        }
        Err(_) => return serde_json::json!({"ok": false, "error": "podcast_keys poisoned"}),
    };

    // Register the per-podcast key in the kernel's identity roster
    // (non-active; FIFO — must arrive before the PublishRaw dispatch and
    // before the blossom upload's kind:24242 sign request).
    register_podcast_signer_in_kernel(handler.app, &secret_hex);

    // Resolve the audio URL. When the episode has a local download AND a
    // configured Blossom server, dispatch the upload to the kernel and
    // use the imeta-enriched tag set (with a placeholder URL that the
    // kernel will overwrite once the upload resolves asynchronously via
    // the action_results slot). On any failure / missing file, fall back
    // to the RSS enclosure URL.
    let (tags, blossom_correlation_id) =
        if local_path.is_some() && !blossom_servers.is_empty() && !handler.app.is_null() {
            let path = local_path.as_deref().unwrap();
            match blossom_upload_via_nmp(handler.app, path, &blossom_servers, &pubkey_hex) {
                Some(corr_id) => {
                    // Blossom upload dispatched — use the enclosure URL as
                    // placeholder; the iOS/Swift layer reads the final URL
                    // from action_results[corr_id] once the upload settles.
                    (episode_to_episode_tags(&episode), Some(corr_id))
                }
                None => (episode_to_episode_tags(&episode), None),
            }
        } else {
            (episode_to_episode_tags(&episode), None)
        };
    let content = episode.description.clone();

    handler.state.infra.rev.fetch_add(1, Ordering::Relaxed);

    // Dispatch PublishRaw signed by the per-podcast key (no app-side signing).
    let status =
        publish_raw_with_signer_via_nmp(handler.app, KIND_EPISODE, &tags, &content, &pubkey_hex);
    serde_json::json!({
        "ok": true,
        "status": status,
        "pubkey_hex": pubkey_hex,
        "event_tags": tags,
        "blossom_correlation_id": blossom_correlation_id,
    })
}

/// `podcast.publish.publish_author_claim` — emit a `kind:10064` author-claim
/// event via `nmp.publish { PublishRaw }`. NMP signs with the active user
/// signer (D4/D7 compliant — no secret bytes in app code for this path).
fn publish_author_claim(
    handler: &PodcastHostOpHandler,
    agent_pubkey_hex: String,
) -> serde_json::Value {
    if agent_pubkey_hex.is_empty() {
        return serde_json::json!({"ok": false, "error": "agent_pubkey_hex is empty"});
    }
    let pairs = match handler.state.publish.podcast_keys.lock() {
        Ok(keys) => keys.iter_pubkeys(),
        Err(_) => return serde_json::json!({"ok": false, "error": "podcast_keys poisoned"}),
    };
    let tags: Vec<Vec<String>> = pairs
        .iter()
        .map(|(_, pk)| vec!["p".into(), pk.clone()])
        .collect();
    handler.state.infra.rev.fetch_add(1, Ordering::Relaxed);
    let status = publish_raw_via_nmp(handler.app, KIND_AUTHOR_CLAIM, &tags, "");
    serde_json::json!({
        "ok": true,
        "status": status,
        "event_tags": tags,
        "owned_count": pairs.len(),
    })
}

/// `podcast.publish.remove_owned_podcast` — drop the per-podcast key,
/// clear `owner_pubkey_hex` from the podcast row, and discard the
/// publish state for that podcast.
fn remove_owned(handler: &PodcastHostOpHandler, podcast_id: String) -> serde_json::Value {
    if let Ok(mut keys) = handler.state.publish.podcast_keys.lock() {
        keys.remove_key(&podcast_id);
    }
    if let Ok(mut s) = handler.state.library.store.lock() {
        s.clear_owner_pubkey_hex(&podcast_id);
    }
    if let Ok(mut state) = handler.state.publish.publish_state.lock() {
        state.remove(&podcast_id);
    }
    handler.state.infra.rev.fetch_add(1, Ordering::Relaxed);
    serde_json::json!({"ok": true})
}

#[cfg(test)]
#[path = "host_op_publish_tests.rs"]
mod tests;
