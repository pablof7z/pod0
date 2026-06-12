//! Owned-podcast update/delete lifecycle handlers for the
//! `podcast.publish.*` action namespace.
//!
//! Split from [`crate::host_op_publish`] to keep that file under the
//! 500-line hard limit (AGENTS.md). The feed-less podcast row itself is
//! created via the first-class `podcast.create_podcast` op (see
//! [`crate::host_op_handler`]); these handlers own the publish-side lifecycle
//! on top of that row:
//!
//! * [`update_owned`] — apply a partial metadata update and re-publish the
//!   `kind:10154` show event when the podcast is public + nostr is enabled.
//!   The publish gate lives here, not in Swift (D7 — the kernel owns it).
//! * [`delete_owned`] — publish a NIP-09 (`kind:5`) deletion for the prior
//!   show event, drop the per-podcast key, and remove the row + episodes.
//!
//! All handlers run on the actor thread (the host-op handler dispatch
//! seam), mirroring the layering in [`crate::host_op_publish`].

use std::sync::atomic::Ordering;

use chrono::Utc;
use nostr::JsonUtil;
use podcast_core::NostrVisibility;
use podcast_discovery::KIND_SHOW;

use crate::ffi::actions::publish_module::PublishAction;
use crate::host_op_handler::PodcastHostOpHandler;
use crate::host_op_publish::{publish_show, sign_event};
use crate::nmp_dispatch::{publish_via_nmp, self_dispatch_publish};

/// NIP-09 deletion request kind.
const KIND_DELETION: u32 = 5;

/// Route the update/delete lifecycle variants of [`PublishAction`].
/// Called from [`crate::host_op_publish::handle_publish_action`] — the
/// destructuring lives here (not in `host_op_publish.rs`) to keep that file
/// under the 500-LOC hard limit. Any non-lifecycle variant is unreachable
/// (the caller only forwards the two lifecycle variants).
pub fn handle_lifecycle_action(
    handler: &PodcastHostOpHandler,
    action: PublishAction,
) -> serde_json::Value {
    match action {
        PublishAction::UpdateOwnedPodcast {
            podcast_id,
            title,
            description,
            author,
            artwork_url,
            visibility,
        } => update_owned(
            handler,
            podcast_id,
            title,
            description,
            author,
            artwork_url,
            visibility,
        ),
        PublishAction::DeleteOwnedPodcast { podcast_id } => delete_owned(handler, podcast_id),
        other => serde_json::json!({
            "ok": false,
            "error": format!("non-lifecycle action routed to lifecycle handler: {other:?}")
        }),
    }
}

/// Parse the canonical `NostrVisibility` snake_case string. Unknown /
/// absent → `Public` (matches the Swift `Podcast.nostrVisibility` default).
fn parse_visibility(raw: Option<String>) -> NostrVisibility {
    match raw.as_deref() {
        Some("private") => NostrVisibility::Private,
        _ => NostrVisibility::Public,
    }
}

/// `podcast.publish.update_owned_podcast` — apply a partial metadata
/// update to an owned podcast row, then re-publish the `kind:10154` show
/// event when the podcast is public AND nostr is enabled. Returns the
/// publish status when a re-publish ran, or `"skipped"` when the gate is
/// closed (private show, or nostr disabled).
///
/// `author` + `visibility` are applied to the kernel row (the SSOT) so the
/// next snapshot push does not revert a Swift-side edit / flip. Because
/// `visibility` is applied *before* the gate is read, a private→public flip
/// republishes the show in the same op.
///
/// On a private→public flip the kernel also backfills every existing episode
/// as a `kind:54` event (D0: Rust owns publish policy end-to-end). Rather than
/// publishing them synchronously in a loop — which would block the actor
/// thread for N sequential Blossom uploads + relay broadcasts and freeze all
/// reactivity (D8 stall) — it **self-enqueues** one `podcast.publish`
/// `publish_episode` action per episode via [`self_dispatch_publish`]. Each
/// lands as its own `ActorCommand::DispatchHostOp` in the actor queue, so the
/// per-episode publish runs in its OWN later tick and the actor yields between
/// them (same responsiveness the old Swift per-episode loop had). The response
/// includes `"episodes_queued": N` (episodes the flip identified for backfill)
/// and `"episodes_accepted": M` (self-dispatches the FFI registry accepted —
/// `M == N` with a live kernel; `0` under a null app in tests). A non-flip
/// update (already-public show) only republishes the show event;
/// `episodes_queued` is 0.
#[allow(clippy::too_many_arguments)]
pub fn update_owned(
    handler: &PodcastHostOpHandler,
    podcast_id: String,
    title: Option<String>,
    description: Option<String>,
    author: Option<String>,
    artwork_url: Option<String>,
    visibility: Option<String>,
) -> serde_json::Value {
    let new_visibility = visibility.map(|v| parse_visibility(Some(v)));
    // Mutate the row, then read the gate inputs under the same lock so the
    // republish decision reflects the just-applied update (visibility first).
    // Also capture whether this is a private→public flip so we can backfill
    // per-episode kind:54 events after the show republish (D0: kernel owns all
    // publish policy).
    let (updated, should_publish, episode_ids_to_backfill) =
        match handler.state.library.store.lock() {
            Ok(mut s) => {
                // Capture pre-update visibility to detect the private→public flip.
                let was_private = s
                    .podcast_by_id_str(&podcast_id)
                    .map(|p| p.nostr_visibility != NostrVisibility::Public)
                    .unwrap_or(false);

                let updated = s.update_owned_metadata(
                    &podcast_id,
                    title,
                    description,
                    author,
                    artwork_url,
                    new_visibility,
                );
                let is_public = s
                    .podcast_by_id_str(&podcast_id)
                    .map(|p| p.nostr_visibility == NostrVisibility::Public)
                    .unwrap_or(false);
                let nostr_enabled = s.nostr_enabled();
                let gate = updated && is_public && nostr_enabled;

                // Collect episode IDs for backfill when flipping private→public.
                // We gather them here (under the lock) and publish after releasing
                // it, since publish_episode re-acquires the store lock.
                let episode_ids = if gate && was_private {
                    s.podcast_by_id_str(&podcast_id)
                        .map(|p| p.id)
                        .map(|pid| {
                            s.episodes_for(pid)
                                .iter()
                                .map(|e| e.id.0.to_string())
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                } else {
                    vec![]
                };

                (updated, gate, episode_ids)
            }
            Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
        };
    if !updated {
        return serde_json::json!({
            "ok": false,
            "error": format!("podcast not found: {podcast_id}")
        });
    }
    handler.state.infra.rev.fetch_add(1, Ordering::Relaxed);

    if !should_publish {
        return serde_json::json!({"ok": true, "status": "skipped"});
    }
    // Reuse the canonical show-event build/sign/broadcast path — do NOT
    // re-implement signing here. `publish_show` stamps the new event JSON
    // onto `publish_state` and bumps `rev`.
    let publish = publish_show(handler, podcast_id);

    // Backfill per-episode kind:54 events on a private→public flip by
    // SELF-ENQUEUING one `publish_episode` action per episode (NOT a
    // synchronous loop). Each self-dispatch enqueues an
    // `ActorCommand::DispatchHostOp` and returns immediately, so each episode
    // publishes in its own later actor tick and the actor yields in between —
    // a 50–100 episode flip stays responsive instead of blocking the actor for
    // N sequential Blossom uploads (D8). The dispatched action reuses the same
    // `publish_episode` path (Blossom upload + sign + broadcast). Lock is NOT
    // held here.
    //
    // `episodes_queued` is the number of episodes the kernel DECIDED to backfill
    // (the policy decision — what this op owns under D0). `episodes_accepted`
    // is how many the FFI registry accepted; in unit tests (null app) the
    // self-dispatch is a no-op so accepted is 0, but the decision count still
    // reflects the flip.
    let episodes_queued = episode_ids_to_backfill.len();
    let mut episodes_accepted = 0usize;
    for episode_id in episode_ids_to_backfill {
        let body = serde_json::json!({ "op": "publish_episode", "episode_id": episode_id });
        if self_dispatch_publish(handler.app, body) {
            episodes_accepted += 1;
        }
    }

    serde_json::json!({
        "ok": true,
        "status": "republished",
        "publish": publish,
        "episodes_queued": episodes_queued,
        "episodes_accepted": episodes_accepted,
    })
}

/// `podcast.publish.delete_owned_podcast` — full owned-podcast deletion:
///
/// 1. Build + sign a NIP-09 `kind:5` deletion event with the *per-podcast*
///    key, referencing the last-published `kind:10154` show event, and
///    broadcast it. This MUST happen before the key is dropped (otherwise
///    we can no longer sign the deletion).
/// 2. Drop the per-podcast key.
/// 3. Remove the podcast row + episodes from the store.
/// 4. Discard the publish state.
pub fn delete_owned(handler: &PodcastHostOpHandler, podcast_id: String) -> serde_json::Value {
    // Resolve the prior show event id (if we ever published one) so the
    // NIP-09 event can reference it. Absent → no published show to delete;
    // we still tear down local state.
    // Step 13: publish_state now in state.publish (PublishState).
    let show_event_id = handler
        .state
        .publish
        .publish_state
        .lock()
        .ok()
        .and_then(|state| {
            state
                .get(&podcast_id)
                .and_then(|s| s.show_event_json.clone())
        })
        .and_then(|json| event_id_from_json(&json));

    // Step 1: NIP-09 deletion (only when there is a published show event AND
    // nostr is enabled — signing a deletion for an event that was never
    // broadcast is pointless).
    let mut deletion_status = "skipped";
    let mut deletion_event_id: Option<String> = None;
    let nostr_enabled = handler
        .state.library.store
        .lock()
        .ok()
        .map(|s: std::sync::MutexGuard<'_, crate::store::PodcastStore>| s.nostr_enabled())
        .unwrap_or(false);
    if let (Some(event_id), true) = (show_event_id.as_ref(), nostr_enabled) {
        // Step 13: podcast_keys now in state.publish (PublishState).
        let secret_bytes = handler
            .state
            .publish
            .podcast_keys
            .lock()
            .ok()
            .and_then(|keys| keys.get_key(&podcast_id).copied());
        if let Some(sk) = secret_bytes {
            let tags = vec![
                vec!["e".to_string(), event_id.clone()],
                vec!["k".to_string(), KIND_SHOW.to_string()],
            ];
            let created_at = Utc::now().timestamp();
            match sign_event(&sk, KIND_DELETION, &tags, "", created_at) {
                Ok((event, ev_id)) => {
                    deletion_status = publish_via_nmp(handler.app, &event);
                    deletion_event_id = Some(ev_id);
                }
                Err(e) => {
                    eprintln!("[host_op_publish_lifecycle] NIP-09 sign failed: {e}");
                    deletion_status = "sign_failed";
                }
            }
        }
    }

    // Step 2: drop the per-podcast key (after signing the deletion).
    // Step 13: podcast_keys now in state.publish (PublishState).
    if let Ok(mut keys) = handler.state.publish.podcast_keys.lock() {
        keys.remove_key(&podcast_id);
    }
    // Step 3: remove the row + episodes.
    if let Ok(mut s) = handler.state.library.store.lock() {
        s.remove_podcast_and_episodes(&podcast_id);
    }
    // Step 4: discard publish state.
    // Step 13: publish_state now in state.publish (PublishState).
    if let Ok(mut state) = handler.state.publish.publish_state.lock() {
        state.remove(&podcast_id);
    }
    handler.state.infra.rev.fetch_add(1, Ordering::Relaxed);

    serde_json::json!({
        "ok": true,
        "deletion_status": deletion_status,
        "deletion_event_id": deletion_event_id,
    })
}

/// Extract the `id` hex from a signed Nostr event JSON string. Returns
/// `None` for unsigned placeholders (the `create_owned`/pre-login path) or
/// malformed JSON.
fn event_id_from_json(json: &str) -> Option<String> {
    nostr::Event::from_json(json).ok().map(|e| e.id.to_hex())
}

#[cfg(test)]
#[path = "host_op_publish_lifecycle_tests.rs"]
mod tests;
