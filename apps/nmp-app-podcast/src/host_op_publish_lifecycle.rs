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

use podcast_core::NostrVisibility;
use podcast_discovery::{KIND_EPISODE, KIND_SHOW};

use crate::ffi::actions::publish_module::PublishAction;
use crate::host_op_handler::PodcastHostOpHandler;
use crate::host_op_publish::publish_show;
use crate::nmp_dispatch::{
    publish_raw_with_signer_to_relays_via_nmp, register_podcast_signer_in_kernel,
    self_dispatch_publish, write_relay_urls,
};
use crate::store::podcast_keys::secret_to_hex;

/// NIP-09 deletion request kind.
const KIND_DELETION: u32 = 5;

/// Build the NIP-09 `k`-tag array for a full podcast deletion.
///
/// Returns a two-element tag list covering BOTH the show kind (10154) and
/// the episode kind (54) so that a single `kind:5` event tombstones the
/// entire per-podcast footprint in one dispatch. Exposed as `pub(crate)` for
/// use in unit tests without a live kernel.
pub(crate) fn deletion_tags() -> Vec<Vec<String>> {
    vec![
        vec!["k".to_string(), KIND_SHOW.to_string()],
        vec!["k".to_string(), KIND_EPISODE.to_string()],
    ]
}

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
/// 1. Dispatch a single NIP-09 `kind:5` deletion event signed by the
///    *per-podcast* key via the kernel's `PublishRaw { signer_pubkey }` seam
///    (D13 — no raw secret bytes in app code). NIP-09 permits multiple `k`
///    tags in one deletion event; the single event carries BOTH
///    `["k","10154"]` (the show) AND `["k","54"]` (all episodes), instructing
///    relays to tombstone the entire footprint of this per-podcast pubkey.
///    Since the per-podcast key authors exactly that podcast's show + episodes,
///    there is no risk of over-deletion. The signed event id is not returned
///    at dispatch time (the kernel owns signing, D13); the kind-targeted form
///    is the correct substitute. This MUST happen before the key is dropped.
/// 2. Drop the per-podcast key.
/// 3. Remove the podcast row + episodes from the store.
/// 4. Discard the publish state.
pub fn delete_owned(handler: &PodcastHostOpHandler, podcast_id: String) -> serde_json::Value {
    // Check whether a show was ever published (last_published_at is set
    // by publish_show on every dispatch). Absent → no published show to
    // delete; we still tear down local state.
    // Step 13: publish_state now in state.publish (PublishState).
    let show_was_published = handler
        .state
        .publish
        .publish_state
        .lock()
        .ok()
        .and_then(|state| state.get(&podcast_id).and_then(|s| s.last_published_at))
        .is_some();

    // Step 1: NIP-09 deletion (only when a show was ever published AND
    // nostr is enabled — signing a deletion for an event that was never
    // broadcast is pointless).
    //
    // The deletion event is signed by the per-podcast NIP-F4 key via the kernel's
    // PublishRaw { signer_pubkey } seam (D13 — no raw secret bytes in app code).
    // The key is re-registered as a non-active signer immediately before the
    // dispatch; the FIFO actor queue guarantees it lands before the sign request.
    //
    // NIP-09 form: a SINGLE kind:5 event with BOTH `["k","10154"]` (show) AND
    // `["k","54"]` (episodes) tags. NIP-09 permits multiple k-tags; relays must
    // delete all events of each listed kind authored by this pubkey. Since the
    // per-podcast key authors exactly this podcast's show + its episodes,
    // the dual-k event tombstones the whole footprint with no over-deletion.
    // The signed event id is not returned at dispatch time (kernel owns signing,
    // D13); the kind-targeted form is the correct substitute.
    let mut deletion_status = "skipped";
    let nostr_enabled = handler
        .state.library.store
        .lock()
        .ok()
        .map(|s: std::sync::MutexGuard<'_, crate::store::PodcastStore>| s.nostr_enabled())
        .unwrap_or(false);
    if show_was_published && nostr_enabled {
        // Step 13: podcast_keys now in state.publish (PublishState).
        let key_pair = handler
            .state
            .publish
            .podcast_keys
            .lock()
            .ok()
            .and_then(|keys| {
                let pk = keys.pubkey_hex(&podcast_id)?;
                let sk = keys.get_key(&podcast_id).copied()?;
                Some((pk, sk))
            });
        if let Some((pubkey_hex, sk)) = key_pair {
            // Single kind:5 deletion covering BOTH kind:10154 (show) and
            // kind:54 (episodes) — tombstones the full per-podcast footprint.
            let tags = deletion_tags();
            let secret_hex = secret_to_hex(&sk);
            register_podcast_signer_in_kernel(handler.app, &secret_hex);
            let relays = write_relay_urls(handler.app);
            deletion_status = publish_raw_with_signer_to_relays_via_nmp(
                handler.app,
                KIND_DELETION,
                &tags,
                "",
                &pubkey_hex,
                &relays,
            );
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
    })
}

#[cfg(test)]
#[path = "host_op_publish_lifecycle_tests.rs"]
mod tests;
