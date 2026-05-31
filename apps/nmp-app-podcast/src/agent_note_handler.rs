//! Feature #44 — Nostr agent-to-agent kind:1 notes threaded via NIP-10.
//!
//! ## What this is
//!
//! Agent-to-agent coordination rides **public kind:1 text notes** (NIP-01)
//! addressed to a peer's pubkey via a `["p", <hex>]` tag and threaded with
//! NIP-10 (`["e", <root>, "", "root"]`). This matches the inbound/outbound
//! wire shape of the Swift reference pipeline
//! (`App/Sources/Services/NostrAgentResponder.swift` +
//! `NostrAgentResponder+Delegation.swift`): the relay subscribe filter is
//! `{kinds:[1], "#p":[my_pubkey]}` and a reply carries
//! `[["e", root, "", "root"], ["p", peer]]`.
//!
//! **NIP-17 (private DMs) is an explicit non-goal** for agent coordination
//! — see the parity matrix (`docs/plan/nmp-feature-parity.md` #44) and
//! BACKLOG `agent-to-agent-kind1`.
//!
//! ## Scope of this slice
//!
//! This handler delivers the **raw send + receive** primitives now that
//! identity, signer, and relay are real:
//!
//! * [`handle_publish_agent_note`] — sign a kind:1 note (optionally a
//!   NIP-10 reply to a `root_event_id`) and broadcast it to the relay.
//!   Returns a `{status: "published" | "signed"}` envelope mirroring
//!   `host_op_publish.rs` (it is a command, not a snapshot mutation).
//! * [`handle_fetch_agent_notes`] — subscribe to `{kinds:[1],
//!   "#p":[my_pubkey]}`, parse inbound notes into [`AgentNoteSummary`],
//!   and write them to the shared `agent_notes_cache`. The snapshot
//!   builder projects the cache onto `PodcastUpdate.agent_notes` (the
//!   reactive push seam — no polling, no pull symbols).
//!
//! ## What this slice deliberately does NOT do (BACKLOG follow-ups)
//!
//! * **No trust gate.** Every inbound note is surfaced with
//!   `trusted: false`. The kind:3 contact list + trust-list primitives
//!   are still scaffold (`social-graph-store-wiring`,
//!   `nostr-conversations-real-projection`); until they are real the Rust
//!   side cannot classify a sender as an approved peer. The iOS shell
//!   must route inbound notes to an approval surface, never auto-respond.
//! * **No LLM responder loop.** The inbound→model→outbound autopilot
//!   (dedup, per-root turn caps, `wtd-end` end-conversation gate, kind:0
//!   profile hydration) stays on the Swift `NostrAgentResponder` path and
//!   is tracked under `agent-to-agent-kind1` in BACKLOG.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use nostr::nips::nip19::ToBech32;
use nostr::{EventBuilder, Keys, Kind, Tag};

use crate::capability::nostr_relay::{
    NostrRelayRequest, NostrRelayResult, NOSTR_RELAY_CAPABILITY_NAMESPACE,
};
use crate::ffi::projections::AgentNoteSummary;
use crate::store::identity::IdentityStore;
use nmp_core::substrate::CapabilityRequest;
use nmp_ffi::NmpApp;

/// Default relay for agent-to-agent note operations. Matches the relay
/// used by comments / NIP-F4 publish so a single connection serves all
/// Nostr traffic.
const AGENT_NOTE_RELAY: &str = "wss://relay.primal.net";

/// Cap on inbound notes parsed per fetch — defence against a flood from
/// the relay overwhelming the snapshot.
const MAX_INBOUND_NOTES: usize = 200;

/// Dispatch a `NostrRelayRequest` via the capability ABI and decode the
/// result. Mirrors `comments_handler::dispatch_nostr_relay`.
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
    // SAFETY: caller holds the same pointer contract as dispatch_http —
    // Swift only dispatches host-ops on the actor thread, and the app
    // pointer outlives the call.
    let envelope = unsafe { &*app }.dispatch_capability(&cap_req);
    serde_json::from_str::<NostrRelayResult>(&envelope.result_json)
        .map_err(|e| format!("decode nostr_relay result: {e}"))
}

/// Build (and sign) a kind:1 agent-to-agent note.
///
/// Tags follow the Swift outbound contract:
/// * a NIP-10 root marker `["e", <root>, "", "root"]` when replying;
/// * a recipient `["p", <recipient_hex>]` so the peer's relay subscription
///   (`#p` filter) delivers it.
///
/// Extracted as a pure function so it can be unit-tested without a relay
/// or a live `NmpApp`.
pub(crate) fn build_agent_note_event(
    keys: &Keys,
    recipient_pubkey_hex: &str,
    content: &str,
    root_event_id: Option<&str>,
) -> Result<nostr::Event, String> {
    // Validate the recipient pubkey up-front so a malformed key is a clean
    // error rather than a silently-dropped tag.
    nostr::PublicKey::parse(recipient_pubkey_hex)
        .map_err(|e| format!("invalid recipient pubkey: {e}"))?;

    let mut tags: Vec<Tag> = Vec::with_capacity(2);
    if let Some(root) = root_event_id {
        if !root.is_empty() {
            // NIP-10 positional root marker (parity with the Swift
            // `["e", peerRootEventID, "", "root"]` reply tag).
            tags.push(
                Tag::parse(["e", root, "", "root"])
                    .map_err(|e| format!("invalid root event id: {e}"))?,
            );
        }
    }
    tags.push(
        Tag::parse(["p", recipient_pubkey_hex])
            .map_err(|e| format!("invalid recipient tag: {e}"))?,
    );

    EventBuilder::new(Kind::TextNote, content)
        .tags(tags)
        .sign_with_keys(keys)
        .map_err(|e| format!("sign: {e}"))
}

/// `podcast.publish_agent_note` — sign a kind:1 note addressed to
/// `recipient_pubkey_hex` and broadcast it to the relay.
///
/// Returns:
/// * `{"ok": true, "status": "published", "event_id": "..."}` — relay accepted.
/// * `{"ok": true, "status": "signed", "event_id": "..."}` — signed but the
///   relay dispatch was skipped (null app pointer in unit tests) or the relay
///   rejected/errored. The event is valid and can be re-broadcast.
/// * `{"ok": false, "error": "..."}` — could not build/sign (no identity,
///   bad recipient, empty content).
pub fn handle_publish_agent_note(
    app: *mut NmpApp,
    identity: &Arc<Mutex<IdentityStore>>,
    recipient_pubkey_hex: &str,
    content: &str,
    root_event_id: Option<&str>,
    correlation_id: &str,
) -> serde_json::Value {
    if content.trim().is_empty() {
        return serde_json::json!({"ok": false, "error": "empty note"});
    }

    let secret_hex = match identity.lock() {
        Ok(id) => match id.secret_hex.clone() {
            Some(s) => s,
            None => return serde_json::json!({"ok": false, "error": "not signed in"}),
        },
        Err(_) => return serde_json::json!({"ok": false, "error": "identity poisoned"}),
    };

    let keys = match Keys::parse(&secret_hex) {
        Ok(k) => k,
        Err(e) => return serde_json::json!({"ok": false, "error": format!("key parse: {e}")}),
    };

    let event = match build_agent_note_event(&keys, recipient_pubkey_hex, content, root_event_id) {
        Ok(ev) => ev,
        Err(e) => return serde_json::json!({"ok": false, "error": e}),
    };
    let event_id = event.id.to_hex();

    // Null-app guard: unit tests run with `app == null_mut()`. Dispatching
    // a capability through a null pointer is UB — return "signed" early.
    if app.is_null() {
        return serde_json::json!({"ok": true, "status": "signed", "event_id": event_id});
    }

    let event_json = match serde_json::to_string(&event) {
        Ok(j) => j,
        Err(e) => return serde_json::json!({"ok": false, "error": format!("serialize: {e}")}),
    };

    let relay_req = NostrRelayRequest::Publish {
        event_json,
        relay_urls: vec![AGENT_NOTE_RELAY.into()],
    };

    let status = match dispatch_nostr_relay(app, &relay_req, correlation_id) {
        Ok(NostrRelayResult::Published { ok: true, .. }) => "published",
        _ => "signed",
    };

    serde_json::json!({"ok": true, "status": status, "event_id": event_id})
}

/// Parse a relay event frame (`serde_json::Value`) into an
/// [`AgentNoteSummary`]. Returns `None` when the event is missing an id
/// (un-projectable). Extracted so the parse can be unit-tested against
/// fixture frames.
pub(crate) fn parse_agent_note(ev: &serde_json::Value) -> Option<AgentNoteSummary> {
    let id = ev["id"].as_str().unwrap_or("");
    if id.is_empty() {
        return None;
    }
    let pubkey_hex = ev["pubkey"].as_str().unwrap_or("");
    let content = ev["content"].as_str().unwrap_or("").to_string();
    let created_at = ev["created_at"].as_i64().unwrap_or(0);

    // Pre-encode pubkey to npub so iOS renders the stub without a bech32 dep.
    let author_npub = nostr::PublicKey::parse(pubkey_hex)
        .ok()
        .and_then(|pk| pk.to_bech32().ok())
        .unwrap_or_else(|| pubkey_hex.to_string());

    // NIP-10 root: prefer the explicit `root` marker, else fall back to the
    // first `e` tag (NIP-10 "deprecated positional" convention).
    let root_event_id = extract_nip10_root(&ev["tags"]);

    Some(AgentNoteSummary {
        id: id.to_string(),
        author_npub,
        content,
        created_at,
        root_event_id,
        // No trust list yet — always untrusted. See module docs.
        trusted: false,
    })
}

/// Extract the NIP-10 conversation root from an event's `tags` array.
///
/// Resolution order (NIP-10):
/// 1. An `["e", <id>, <relay?>, "root"]` marked tag wins.
/// 2. Otherwise, the first `["e", <id>, ...]` tag (positional convention).
/// 3. `None` when there is no `e` tag (a thread-opening note).
fn extract_nip10_root(tags: &serde_json::Value) -> Option<String> {
    let arr = tags.as_array()?;
    let mut first_e: Option<String> = None;
    for tag in arr {
        let parts = match tag.as_array() {
            Some(p) => p,
            None => continue,
        };
        if parts.first().and_then(|v| v.as_str()) != Some("e") {
            continue;
        }
        let id = match parts.get(1).and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => continue,
        };
        // Marked "root" wins immediately.
        if parts.get(3).and_then(|v| v.as_str()) == Some("root") {
            return Some(id);
        }
        if first_e.is_none() {
            first_e = Some(id);
        }
    }
    first_e
}

/// Turn a batch of raw relay event frames into the projected inbound-note
/// list: drop self-authored events (`pubkey == my_pubkey_hex`), parse the
/// rest, and sort newest-first so the iOS shell renders the most recent
/// note at the top.
///
/// Extracted as a pure function so the filter/sort/dedup contract can be
/// unit-tested without a live relay (the live round-trip in
/// `scenarios/agent_notes.rs` self-addresses, so it can't exercise the
/// foreign-vs-self split).
pub(crate) fn parse_inbound_notes(
    events: &[serde_json::Value],
    my_pubkey_hex: &str,
) -> Vec<AgentNoteSummary> {
    // Drop self-authored notes — never surface our own broadcasts as
    // inbound. (Defence-in-depth; the `#p` filter shouldn't return them
    // unless we tagged ourselves, but a self-reply would.)
    let mut notes: Vec<AgentNoteSummary> = events
        .iter()
        .filter(|ev| ev["pubkey"].as_str() != Some(my_pubkey_hex))
        .filter_map(parse_agent_note)
        .collect();
    // Newest-first so iOS renders the most recent note at the top.
    notes.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    notes
}

/// `podcast.fetch_agent_notes` — subscribe to inbound kind:1 notes
/// addressed to the active account (`#p` filter), parse them, and write
/// the result into `agent_notes_cache`. The snapshot builder projects the
/// cache onto `PodcastUpdate.agent_notes`.
///
/// Returns `{"ok": true, "count": N}` on success; `{"ok": false, ...}` on
/// failure (no identity, relay error, poisoned cache).
pub fn handle_fetch_agent_notes(
    app: *mut NmpApp,
    identity: &Arc<Mutex<IdentityStore>>,
    agent_notes_cache: &Arc<Mutex<Vec<AgentNoteSummary>>>,
    rev: &Arc<std::sync::atomic::AtomicU64>,
    correlation_id: &str,
) -> serde_json::Value {
    let my_pubkey_hex = match identity.lock() {
        Ok(id) => match id.pubkey_hex.clone() {
            Some(p) => p,
            None => return serde_json::json!({"ok": false, "error": "not signed in"}),
        },
        Err(_) => return serde_json::json!({"ok": false, "error": "identity poisoned"}),
    };

    // Null-app guard: unit tests run without a live capability host.
    if app.is_null() {
        return serde_json::json!({"ok": false, "error": "no app pointer"});
    }

    let filter = serde_json::json!({
        "kinds": [1],
        "#p": [my_pubkey_hex],
        "limit": MAX_INBOUND_NOTES,
    });

    let relay_req = NostrRelayRequest::Subscribe {
        sub_id: "agent-notes-inbox".to_string(),
        filter,
        relay_urls: vec![AGENT_NOTE_RELAY.into()],
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

    let notes = parse_inbound_notes(&events, &my_pubkey_hex);
    let count = notes.len();

    match agent_notes_cache.lock() {
        Ok(mut cache) => *cache = notes,
        Err(_) => return serde_json::json!({"ok": false, "error": "agent_notes_cache poisoned"}),
    }

    rev.fetch_add(1, Ordering::Relaxed);
    serde_json::json!({"ok": true, "count": count})
}

#[cfg(test)]
#[path = "agent_note_handler_tests.rs"]
mod tests;
