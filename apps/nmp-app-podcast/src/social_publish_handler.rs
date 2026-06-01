//! User-identity social publishing — kind:0 (profile), kind:1 (note),
//! and kind:9802 (NIP-84 highlight) signing on behalf of the signed-in
//! user.
//!
//! ## What this is
//!
//! These handlers move the kind:0/1/9802 signing that previously lived in
//! Swift (`UserIdentityStore+Publishing.swift`) into the Rust kernel. Each
//! handler reads the active signing key from the podcast-app
//! [`IdentityStore`] (populated by `podcast.identity` `ImportNsec`), builds
//! and signs the event with `secp256k1` via `nostr::Keys` — the same
//! mechanism as [`crate::agent_note_handler`] — and broadcasts it through
//! the kernel's Nostr relay capability.
//!
//! ## Signing key source — local key only
//!
//! The podcast-app [`IdentityStore`] holds the secret key as
//! `secret_hex`, so it can only sign for **local-key** identities. A
//! NIP-46 *bunker* identity keeps the secret key remote (on Amber /
//! nsec.app / …) and is **never** materialised here, so `secret_hex` is
//! `None` and these handlers return `{"ok": false, "error": "not signed
//! in"}`. The iOS shell therefore routes only `.localKey` mode through
//! `podcast.social`; `.remoteSigner` (bunker) publishing stays on the
//! Swift NIP-46 path until a kernel remote-sign seam exists. Tracked in
//! `docs/BACKLOG.md` (`social-bunker-signing-kernel`).
//!
//! ## Null-app guard
//!
//! Unit tests run with `app == null_mut()`. Dispatching a capability
//! through a null pointer is UB, so every handler short-circuits to
//! `{"ok": true, "status": "signed", "event_id": "..."}` once the event
//! is built + signed but before any relay dispatch.

use std::sync::{Arc, Mutex};

use nostr::{EventBuilder, Keys, Kind, Tag};
use serde_json::json;

use crate::capability::nostr_relay::{
    NostrRelayRequest, NostrRelayResult, NOSTR_RELAY_CAPABILITY_NAMESPACE,
};
use crate::store::identity::IdentityStore;
use nmp_core::substrate::CapabilityRequest;
use nmp_ffi::NmpApp;

/// NIP-84 highlight event kind.
const KIND_HIGHLIGHT: u16 = 9802;

/// Default relay for user social publishing. Matches the kernel's primary
/// relay (`agent_note_handler` / `social_handler`) so a single connection
/// serves all outbound Nostr traffic.
const SOCIAL_RELAY: &str = "wss://relay.primal.net";

/// Dispatch a `NostrRelayRequest` via the capability ABI and decode the
/// result. Mirrors `agent_note_handler::dispatch_nostr_relay`.
fn dispatch_nostr_relay(
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
    // SAFETY: caller holds the same pointer contract as the rest of the
    // host-op handlers — Swift only dispatches on the actor thread and the
    // app pointer outlives the call. Callers guard `app.is_null()` first.
    let envelope = unsafe { &*app }.dispatch_capability(&cap_req);
    serde_json::from_str::<NostrRelayResult>(&envelope.result_json)
        .map_err(|e| format!("decode nostr_relay result: {e}"))
}

/// Read the active local signing key from the identity store. Returns the
/// `{"ok": false, ...}` error envelope (already shaped for return) on any
/// failure so callers can `?`-style early-return.
fn signing_keys(identity: &Arc<Mutex<IdentityStore>>) -> Result<Keys, serde_json::Value> {
    let secret_hex = match identity.lock() {
        Ok(id) => match id.secret_hex.clone() {
            Some(s) => s,
            None => return Err(json!({"ok": false, "error": "not signed in"})),
        },
        Err(_) => return Err(json!({"ok": false, "error": "identity poisoned"})),
    };
    Keys::parse(&secret_hex).map_err(|e| json!({"ok": false, "error": format!("key parse: {e}")}))
}

/// Sign + (when an app pointer is present) publish `event`, returning the
/// shared `{status: published|signed}` envelope. Factored out so all three
/// publish handlers share one null-app guard + relay-dispatch path.
fn publish_signed_event(
    app: *mut NmpApp,
    event: nostr::Event,
    correlation_id: &str,
) -> serde_json::Value {
    let event_id = event.id.to_hex();

    // Null-app guard: unit tests run with `app == null_mut()`.
    if app.is_null() {
        return json!({"ok": true, "status": "signed", "event_id": event_id});
    }

    let event_json = match serde_json::to_string(&event) {
        Ok(j) => j,
        Err(e) => return json!({"ok": false, "error": format!("serialize: {e}")}),
    };

    let relay_req = NostrRelayRequest::Publish {
        event_json,
        relay_urls: vec![SOCIAL_RELAY.into()],
    };

    let status = match dispatch_nostr_relay(app, &relay_req, correlation_id) {
        Ok(NostrRelayResult::Published { ok: true, .. }) => "published",
        _ => "signed",
    };

    json!({"ok": true, "status": status, "event_id": event_id})
}

// ── kind:0 profile ───────────────────────────────────────────────────

/// Build (and sign) a kind:0 metadata event. The content is a JSON object
/// carrying `name` plus any present optional fields. Extracted as a pure
/// function so the content shape can be unit-tested without a relay.
pub(crate) fn build_profile_event(
    keys: &Keys,
    name: &str,
    display_name: Option<&str>,
    about: Option<&str>,
    picture: Option<&str>,
) -> Result<nostr::Event, String> {
    let mut payload = serde_json::Map::new();
    payload.insert("name".to_string(), json!(name));
    if let Some(v) = display_name {
        payload.insert("display_name".to_string(), json!(v));
    }
    if let Some(v) = about {
        payload.insert("about".to_string(), json!(v));
    }
    if let Some(v) = picture {
        payload.insert("picture".to_string(), json!(v));
    }
    let content = serde_json::to_string(&serde_json::Value::Object(payload))
        .map_err(|e| format!("serialize profile: {e}"))?;
    EventBuilder::new(Kind::Metadata, content)
        .sign_with_keys(keys)
        .map_err(|e| format!("sign: {e}"))
}

/// `podcast.social` `publish_profile` — sign a kind:0 metadata event with
/// the supplied profile fields and broadcast it.
#[allow(clippy::too_many_arguments)]
pub fn handle_publish_profile(
    app: *mut NmpApp,
    identity: &Arc<Mutex<IdentityStore>>,
    name: &str,
    display_name: Option<&str>,
    about: Option<&str>,
    picture: Option<&str>,
    correlation_id: &str,
) -> serde_json::Value {
    let keys = match signing_keys(identity) {
        Ok(k) => k,
        Err(e) => return e,
    };
    let event = match build_profile_event(&keys, name, display_name, about, picture) {
        Ok(ev) => ev,
        Err(e) => return json!({"ok": false, "error": e}),
    };
    publish_signed_event(app, event, correlation_id)
}

// ── kind:1 note ──────────────────────────────────────────────────────

/// Build (and sign) a kind:1 text note carrying the supplied free-form
/// tags verbatim. Extracted so the tag/kind contract can be unit-tested.
pub(crate) fn build_note_event(
    keys: &Keys,
    content: &str,
    tags: Option<&Vec<Vec<String>>>,
) -> Result<nostr::Event, String> {
    let parsed_tags = parse_tags(tags)?;
    EventBuilder::new(Kind::TextNote, content)
        .tags(parsed_tags)
        .sign_with_keys(keys)
        .map_err(|e| format!("sign: {e}"))
}

/// `podcast.social` `publish_note` — sign a kind:1 text note and broadcast
/// it. Rejects empty content (parity with `agent_note_handler`).
pub fn handle_publish_note(
    app: *mut NmpApp,
    identity: &Arc<Mutex<IdentityStore>>,
    content: &str,
    tags: Option<&Vec<Vec<String>>>,
    correlation_id: &str,
) -> serde_json::Value {
    if content.trim().is_empty() {
        return json!({"ok": false, "error": "empty note"});
    }
    let keys = match signing_keys(identity) {
        Ok(k) => k,
        Err(e) => return e,
    };
    let event = match build_note_event(&keys, content, tags) {
        Ok(ev) => ev,
        Err(e) => return json!({"ok": false, "error": e}),
    };
    publish_signed_event(app, event, correlation_id)
}

// ── kind:9802 highlight (NIP-84) ─────────────────────────────────────

/// Build (and sign) a kind:9802 NIP-84 highlight carrying the supplied
/// free-form tags verbatim. The caller (Swift `publishUserClip`) assembles
/// the full NIP-73 / NIP-84 tag set — `["r", enclosure]`, `["r", feed]`,
/// `["i", "podcast:item:guid:<guid>#t=<start>,<end>"]`, `["context", ...]`,
/// `["alt", ...]` — since it holds the resolved episode + podcast models.
/// Tag *assembly* staying Swift-side is not a D7 violation; only signing
/// moves to the kernel. (BACKLOG `nip73-formatting-kernel` tracks moving
/// the formatting kernel-side if it ever needs to.)
pub(crate) fn build_highlight_event(
    keys: &Keys,
    content: &str,
    tags: Option<&Vec<Vec<String>>>,
) -> Result<nostr::Event, String> {
    let parsed_tags = parse_tags(tags)?;
    EventBuilder::new(Kind::Custom(KIND_HIGHLIGHT), content)
        .tags(parsed_tags)
        .sign_with_keys(keys)
        .map_err(|e| format!("sign: {e}"))
}

/// `podcast.social` `publish_highlight` — sign a kind:9802 NIP-84
/// highlight and broadcast it. Rejects empty content.
pub fn handle_publish_highlight(
    app: *mut NmpApp,
    identity: &Arc<Mutex<IdentityStore>>,
    content: &str,
    tags: Option<&Vec<Vec<String>>>,
    correlation_id: &str,
) -> serde_json::Value {
    if content.trim().is_empty() {
        return json!({"ok": false, "error": "empty highlight"});
    }
    let keys = match signing_keys(identity) {
        Ok(k) => k,
        Err(e) => return e,
    };
    let event = match build_highlight_event(&keys, content, tags) {
        Ok(ev) => ev,
        Err(e) => return json!({"ok": false, "error": e}),
    };
    publish_signed_event(app, event, correlation_id)
}

// ── shared helpers ───────────────────────────────────────────────────

/// Parse caller-supplied `[["t","note"], ...]` string tags into
/// `nostr::Tag`s. A tag that fails `Tag::parse` aborts with an error
/// rather than being silently dropped (the caller controls the tag shape).
fn parse_tags(tags: Option<&Vec<Vec<String>>>) -> Result<Vec<Tag>, String> {
    let Some(tags) = tags else {
        return Ok(Vec::new());
    };
    tags.iter()
        .map(|t| Tag::parse(t.iter().map(String::as_str)).map_err(|e| format!("invalid tag: {e}")))
        .collect()
}

#[cfg(test)]
#[path = "social_publish_handler_tests.rs"]
mod tests;
