//! Owned-podcast create/update/delete lifecycle handlers for the
//! `podcast.publish.*` action namespace.
//!
//! Split from [`crate::host_op_publish`] to keep that file under the
//! 500-line hard limit (AGENTS.md). These handlers make the Rust kernel
//! the single source of truth for synthetic ("owned") podcasts:
//!
//! * [`create_synthetic`] — insert the feed-less podcast row from full
//!   agent-supplied metadata. Until this landed, `create_owned_podcast`
//!   and `publish_show` silently no-op'd for synthetic podcasts because
//!   the row only ever existed in the Swift render store.
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
use crate::host_op_publish::{publish_show, publish_via_nmp, sign_event};
use crate::store::owned_ext::SyntheticChapter;

/// NIP-09 deletion request kind.
const KIND_DELETION: u32 = 5;

/// Route the create/update/delete lifecycle variants of [`PublishAction`].
/// Called from [`crate::host_op_publish::handle_publish_action`] — the
/// destructuring lives here (not in `host_op_publish.rs`) to keep that file
/// under the 500-LOC hard limit. Any non-lifecycle variant is unreachable
/// (the caller only forwards the three lifecycle variants).
pub fn handle_lifecycle_action(
    handler: &PodcastHostOpHandler,
    action: PublishAction,
) -> serde_json::Value {
    match action {
        PublishAction::CreateSyntheticPodcast {
            podcast_id,
            title,
            description,
            author,
            artwork_url,
            language,
            categories,
            visibility,
        } => create_synthetic(
            handler,
            podcast_id,
            title,
            description,
            author,
            artwork_url,
            language,
            categories,
            visibility,
        ),
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
        PublishAction::RegisterSyntheticEpisode {
            podcast_id,
            episode_id,
            title,
            audio_path,
            duration_secs,
            chapters,
            transcript,
        } => register_synthetic_episode(
            handler,
            podcast_id,
            episode_id,
            title,
            audio_path,
            duration_secs,
            chapters,
            transcript,
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

/// `podcast.publish.create_synthetic_podcast` — insert a synthetic
/// (feed-less) podcast row into the kernel store from full metadata so the
/// Rust store is the SSOT for owned podcasts. Idempotent on `podcast_id`.
#[allow(clippy::too_many_arguments)]
pub fn create_synthetic(
    handler: &PodcastHostOpHandler,
    podcast_id: String,
    title: String,
    description: String,
    author: String,
    artwork_url: Option<String>,
    language: Option<String>,
    categories: Vec<String>,
    visibility: Option<String>,
) -> serde_json::Value {
    let visibility = parse_visibility(visibility);
    let inserted = match handler.store.lock() {
        Ok(mut s) => s.insert_synthetic_podcast(
            &podcast_id,
            title,
            description,
            author,
            artwork_url,
            language,
            categories,
            visibility,
        ),
        Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
    };
    if !inserted {
        return serde_json::json!({
            "ok": false,
            "error": format!("invalid podcast id: {podcast_id}")
        });
    }
    handler.rev.fetch_add(1, Ordering::Relaxed);
    serde_json::json!({"ok": true})
}

/// `podcast.publish.register_synthetic_episode` — insert an agent-generated
/// episode (TTS composer output) into the kernel store under a synthetic
/// podcast so the kernel is the SSOT: the episode survives the projection
/// full-replace tick, and `publish_episode` can later resolve it by id.
///
/// Returns `ok: false` when the parent podcast row does not exist, or when the
/// episode id / audio path is unusable (mirrors `insert_synthetic_episode`'s
/// no-op cases). Bumps `rev` on success so the projection picks the episode up.
#[allow(clippy::too_many_arguments)]
pub fn register_synthetic_episode(
    handler: &PodcastHostOpHandler,
    podcast_id: String,
    episode_id: String,
    title: String,
    audio_path: String,
    duration_secs: Option<f64>,
    chapters: Vec<crate::ffi::actions::publish_module::SyntheticChapterArg>,
    transcript: Option<String>,
) -> serde_json::Value {
    let chapters: Vec<SyntheticChapter> = chapters
        .into_iter()
        .map(|c| SyntheticChapter {
            start_secs: c.start_secs,
            title: c.title,
            image_url: c.image_url,
            source_episode_id: c.source_episode_id,
        })
        .collect();

    let inserted = match handler.store.lock() {
        Ok(mut s) => s.insert_synthetic_episode(
            &podcast_id,
            &episode_id,
            title,
            &audio_path,
            duration_secs,
            chapters,
            transcript,
        ),
        Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
    };
    if !inserted {
        return serde_json::json!({
            "ok": false,
            "error": format!("could not register episode {episode_id} under podcast {podcast_id}")
        });
    }
    handler.rev.fetch_add(1, Ordering::Relaxed);
    serde_json::json!({"ok": true, "episode_id": episode_id})
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
    let visibility = visibility.map(|v| parse_visibility(Some(v)));
    // Mutate the row, then read the gate inputs under the same lock so the
    // republish decision reflects the just-applied update (visibility first).
    let (updated, should_publish) = match handler.store.lock() {
        Ok(mut s) => {
            let updated = s.update_owned_metadata(
                &podcast_id,
                title,
                description,
                author,
                artwork_url,
                visibility,
            );
            let is_public = s
                .podcast_by_id_str(&podcast_id)
                .map(|p| p.nostr_visibility == NostrVisibility::Public)
                .unwrap_or(false);
            let gate = updated && is_public && s.nostr_enabled();
            (updated, gate)
        }
        Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
    };
    if !updated {
        return serde_json::json!({
            "ok": false,
            "error": format!("podcast not found: {podcast_id}")
        });
    }
    handler.rev.fetch_add(1, Ordering::Relaxed);

    if !should_publish {
        return serde_json::json!({"ok": true, "status": "skipped"});
    }
    // Reuse the canonical show-event build/sign/broadcast path — do NOT
    // re-implement signing here. `publish_show` stamps the new event JSON
    // onto `publish_state` and bumps `rev`.
    let publish = publish_show(handler, podcast_id);
    serde_json::json!({
        "ok": true,
        "status": "republished",
        "publish": publish,
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
    let show_event_id = handler
        .publish_state
        .lock()
        .ok()
        .and_then(|state| state.get(&podcast_id).and_then(|s| s.show_event_json.clone()))
        .and_then(|json| event_id_from_json(&json));

    // Step 1: NIP-09 deletion (only when there is a published show event AND
    // nostr is enabled — signing a deletion for an event that was never
    // broadcast is pointless).
    let mut deletion_status = "skipped";
    let mut deletion_event_id: Option<String> = None;
    let nostr_enabled = handler
        .store
        .lock()
        .ok()
        .map(|s| s.nostr_enabled())
        .unwrap_or(false);
    if let (Some(event_id), true) = (show_event_id.as_ref(), nostr_enabled) {
        let secret_bytes = handler
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
    if let Ok(mut keys) = handler.podcast_keys.lock() {
        keys.remove_key(&podcast_id);
    }
    // Step 3: remove the row + episodes.
    if let Ok(mut s) = handler.store.lock() {
        s.remove_podcast_and_episodes(&podcast_id);
    }
    // Step 4: discard publish state.
    if let Ok(mut state) = handler.publish_state.lock() {
        state.remove(&podcast_id);
    }
    handler.rev.fetch_add(1, Ordering::Relaxed);

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
    nostr::Event::from_json(json)
        .ok()
        .map(|e| e.id.to_hex())
}

#[cfg(test)]
#[path = "host_op_publish_lifecycle_tests.rs"]
mod tests;
