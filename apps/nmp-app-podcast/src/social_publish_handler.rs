//! User-identity social publishing — kind:0 (profile), kind:1 (note),
//! and kind:9802 (NIP-84 highlight) for the signed-in user.
//!
//! ## What this is
//!
//! These handlers publish the user's social events through the kernel's
//! active-account signer. No secret bytes and no `nostr::Event` construction
//! live here: each handler assembles plain `{kind, tags, content}` (or the
//! kind:0 `fields` map) and dispatches `nmp.publish { PublishRaw }` /
//! `nmp.publish { PublishProfile }`. NMP fills `pubkey` from the active
//! account, stamps `created_at` (D7 — kernel owns the wall clock), signs with
//! the active signer (local nsec OR NIP-46 bunker — both are handled by the
//! kernel), and routes through the NIP-65 outbox (D3).
//!
//! ## Active-account requirement
//!
//! Publishing requires a signed-in account. The kernel owns the signer; this
//! handler guards on the podcast-app [`IdentityStore`]'s `pubkey_hex` (mirror
//! of the signed-in state) and returns `{"ok": false, "error": "not signed
//! in"}` when empty. Both local-key and bunker identities publish through the
//! same kernel path now — the prior "local key only" limitation is gone since
//! signing no longer happens in app code.
//!
//! ## Null-app guard
//!
//! Unit tests run with `app == null_mut()`. The dispatch helpers in
//! [`crate::nmp_dispatch`] short-circuit to `"signed"` under a null pointer,
//! so the handlers return `{"ok": true, "status": "signed"}` without touching
//! the FFI boundary.

use std::sync::{Arc, Mutex};

use serde_json::json;

use crate::clip_handler::ClipRecord;
use crate::nmp_dispatch::{publish_profile_via_nmp, publish_raw_via_nmp};
use crate::store::identity::IdentityStore;
use crate::store::PodcastStore;
use nmp_ffi::NmpApp;

/// NIP-84 highlight event kind.
const KIND_HIGHLIGHT: u32 = 9802;
/// NIP-01 text note event kind.
const KIND_TEXT_NOTE: u32 = 1;

/// Guard: the active account must be signed in. Returns the `{"ok": false}`
/// error envelope (already shaped for return) when not, so callers early-return.
fn require_signed_in(identity: &Arc<Mutex<IdentityStore>>) -> Result<(), serde_json::Value> {
    match identity.lock() {
        Ok(id) if id.pubkey_hex.is_some() => Ok(()),
        Ok(_) => Err(json!({"ok": false, "error": "not signed in"})),
        Err(_) => Err(json!({"ok": false, "error": "identity poisoned"})),
    }
}

// ── kind:0 profile ───────────────────────────────────────────────────

/// Assemble the kind:0 `fields` map from the supplied profile values.
/// Pure helper so the field shape can be unit-tested without a kernel.
pub(crate) fn build_profile_fields(
    name: &str,
    display_name: Option<&str>,
    about: Option<&str>,
    picture: Option<&str>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut fields = serde_json::Map::new();
    fields.insert("name".to_string(), json!(name));
    if let Some(v) = display_name {
        fields.insert("display_name".to_string(), json!(v));
    }
    if let Some(v) = about {
        fields.insert("about".to_string(), json!(v));
    }
    if let Some(v) = picture {
        fields.insert("picture".to_string(), json!(v));
    }
    fields
}

/// `podcast.social` `publish_profile` — publish a kind:0 metadata event with
/// the supplied profile fields via the kernel's active-account signer.
///
/// After the publish is dispatched successfully, the just-published fields are
/// mirrored into the local `IdentityStore` via `apply_profile` so the
/// `AccountSummary` projection reflects the new values immediately — without
/// waiting for a relay echo (optimistic-but-correct; see
/// `agent_note_responder`'s projection-slot update for the established
/// precedent).
///
/// The caller (`social_actions.rs`) must bump `Domain::Identity` after this
/// returns `ok: true` so the identity push-frame re-emits with the fresh
/// `AccountSummary`.
#[allow(clippy::too_many_arguments)]
pub fn handle_publish_profile(
    app: *mut NmpApp,
    identity: &Arc<Mutex<IdentityStore>>,
    name: &str,
    display_name: Option<&str>,
    about: Option<&str>,
    picture: Option<&str>,
    _correlation_id: &str,
) -> serde_json::Value {
    if let Err(e) = require_signed_in(identity) {
        return e;
    }
    let fields = build_profile_fields(name, display_name, about, picture);
    let status = publish_profile_via_nmp(app, fields);

    // Self-apply the published profile to the local IdentityStore so the
    // AccountSummary projection reflects it immediately. Only apply fields the
    // caller actually supplied — don't null-out existing values. `picture`
    // (payload field name) maps to `picture_url` (store field name).
    if let Ok(mut id) = identity.lock() {
        id.apply_profile(
            display_name.map(str::to_owned),
            picture.map(str::to_owned),
        );
    }

    json!({"ok": true, "status": status})
}

// ── kind:1 note ──────────────────────────────────────────────────────

/// Build the NIP tags for a kind:1 user note from typed fields. An optional
/// `["a", episode_coord]` reference precedes the `["t","note"]` marker
/// (preserving the prior Swift-side ordering). Pure so it can be unit-tested
/// without a kernel.
pub(crate) fn build_note_tags(episode_coord: Option<&str>) -> Vec<Vec<String>> {
    let mut tags: Vec<Vec<String>> = Vec::new();
    if let Some(coord) = episode_coord {
        if !coord.is_empty() {
            tags.push(vec!["a".to_string(), coord.to_string()]);
        }
    }
    tags.push(vec!["t".to_string(), "note".to_string()]);
    tags
}

/// `podcast.social` `publish_note` — publish a kind:1 text note. The kernel
/// builds the NIP tags from `episode_coord` (Nostr tag semantics belong in the
/// kernel, not the shell). Rejects empty content (parity with
/// `agent_note_handler`).
pub fn handle_publish_note(
    app: *mut NmpApp,
    identity: &Arc<Mutex<IdentityStore>>,
    content: &str,
    episode_coord: Option<&str>,
    _correlation_id: &str,
) -> serde_json::Value {
    if content.trim().is_empty() {
        return json!({"ok": false, "error": "empty note"});
    }
    if let Err(e) = require_signed_in(identity) {
        return e;
    }
    let tags = build_note_tags(episode_coord);
    let status = publish_raw_via_nmp(app, KIND_TEXT_NOTE, &tags, content);
    json!({"ok": true, "status": status})
}

// ── kind:9802 highlight (NIP-84) ─────────────────────────────────────

/// Typed inputs for a kind:9802 highlight — the resolved episode/podcast
/// values the shell holds, passed instead of pre-built tags.
pub(crate) struct HighlightFields<'a> {
    /// NIP-84 source URL — the audio enclosure (`["r", …]`).
    pub enclosure_url: Option<&'a str>,
    /// NIP-73 podcast feed reference (`["r", …]`).
    pub feed_url: Option<&'a str>,
    /// NIP-73 episode guid for the `["i", "podcast:item:guid:<guid>…"]` tag.
    pub item_guid: Option<&'a str>,
    /// Media-fragment start/end offsets (seconds) appended to the `i` tag.
    pub start_sec: Option<i64>,
    pub end_sec: Option<i64>,
    /// Human-readable caption (`["alt", …]`) when present.
    pub caption: Option<&'a str>,
}

/// Build the NIP-73 / NIP-84 tag set for a kind:9802 highlight from typed
/// fields. `content` (the highlighted text) doubles as the `["context", …]`
/// value, matching the prior Swift behavior. Tag order: `r`(enclosure),
/// `r`(feed), `i`(item guid + time fragment), `context`, `alt`(caption).
/// Pure so it can be unit-tested without a kernel.
pub(crate) fn build_highlight_tags(content: &str, f: &HighlightFields) -> Vec<Vec<String>> {
    let mut tags: Vec<Vec<String>> = Vec::new();
    if let Some(url) = f.enclosure_url {
        tags.push(vec!["r".to_string(), url.to_string()]);
    }
    if let Some(url) = f.feed_url {
        tags.push(vec!["r".to_string(), url.to_string()]);
    }
    if let Some(guid) = f.item_guid {
        let start = f.start_sec.unwrap_or(0);
        let end = f.end_sec.unwrap_or(0);
        tags.push(vec![
            "i".to_string(),
            format!("podcast:item:guid:{guid}#t={start},{end}"),
        ]);
    }
    tags.push(vec!["context".to_string(), content.to_string()]);
    if let Some(caption) = f.caption {
        if !caption.is_empty() {
            tags.push(vec!["alt".to_string(), caption.to_string()]);
        }
    }
    tags
}

/// `podcast.social` `publish_highlight` — publish a kind:9802 NIP-84
/// highlight. The kernel assembles the NIP-73 / NIP-84 tag set from typed
/// fields (Nostr tag semantics belong in the kernel, per the codebase's own
/// "Swift passes typed values; Rust builds tags" convention). Rejects empty
/// content.
pub fn handle_publish_highlight(
    app: *mut NmpApp,
    identity: &Arc<Mutex<IdentityStore>>,
    content: &str,
    fields: &HighlightFields,
    _correlation_id: &str,
) -> serde_json::Value {
    if content.trim().is_empty() {
        return json!({"ok": false, "error": "empty highlight"});
    }
    if let Err(e) = require_signed_in(identity) {
        return e;
    }
    let tags = build_highlight_tags(content, fields);
    let status = publish_raw_via_nmp(app, KIND_HIGHLIGHT, &tags, content);
    json!({"ok": true, "status": status})
}

/// Publish a Rust-owned clip as a kind:9802 highlight when it is user-visible
/// and has transcript text. Agent-created clips stay local; pending clips with
/// no transcript wait until transcript refinement supplies content.
pub(crate) fn publish_clip_highlight_if_user_visible(
    app: *mut NmpApp,
    identity: &Arc<Mutex<IdentityStore>>,
    store: &Arc<Mutex<PodcastStore>>,
    clip: &ClipRecord,
    correlation_id: &str,
) {
    if clip.source == "agent" || clip.transcript_text.trim().is_empty() {
        return;
    }
    let Some((enclosure_url, feed_url, item_guid)) = store
        .lock()
        .ok()
        .and_then(|store| store.episode_highlight_metadata(&clip.episode_id))
    else {
        return;
    };
    let fields = HighlightFields {
        enclosure_url: Some(enclosure_url.as_str()),
        feed_url: feed_url.as_deref(),
        item_guid: Some(item_guid.as_str()),
        start_sec: Some(clip.start_secs.round() as i64),
        end_sec: Some(clip.end_secs.round() as i64),
        caption: clip.title.as_deref(),
    };
    let _ = handle_publish_highlight(
        app,
        identity,
        &clip.transcript_text,
        &fields,
        correlation_id,
    );
}

#[cfg(test)]
#[path = "social_publish_handler_tests.rs"]
mod tests;
