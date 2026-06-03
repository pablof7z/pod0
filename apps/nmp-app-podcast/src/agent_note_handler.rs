//! Feature #44 — Nostr agent-to-agent kind:1 notes threaded via NIP-10.
//!
//! ## Relay
//!
//! Publish: `nmp.publish { PublishRaw }` — NMP signs with active user signer
//! and routes through its relay pool. No iOS WebSocket, no relay URLs in app.
//!
//! Subscribe: `push_interest_via_nmp` with `kind:1` + `#p` tag filter and
//! `InterestLifecycle::OneShot`. NMP opens the subscription; events arrive via
//! [`AgentNotesObserver`] registered at init.
//!
//! ## What this slice deliberately does NOT do (BACKLOG follow-ups)
//!
//! * **No trust gate.** Every inbound note is surfaced with `trusted: false`.
//! * **No LLM responder loop.** Still on the Swift `NostrAgentResponder` path.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use nostr::nips::nip19::ToBech32;
use nostr::{Tag};

use nmp_core::planner::{InterestId, InterestLifecycle, InterestScope, LogicalInterest};
use nmp_core::stable_hash::stable_hash64;
use nmp_core::substrate::{KernelEvent, ViewDependencies};
use nmp_core::KernelEventObserver;

use crate::ffi::projections::AgentNoteSummary;
use crate::nmp_dispatch::{publish_raw_via_nmp, push_interest_via_nmp};
use crate::store::identity::IdentityStore;
use nmp_ffi::NmpApp;

const MAX_INBOUND_NOTES: usize = 200;

// ── subscribe helpers ────────────────────────────────────────────────────────

fn agent_notes_interest(my_pubkey_hex: &str) -> LogicalInterest {
    ViewDependencies {
        kinds: vec![1],
        tag_refs: vec![("p".to_string(), my_pubkey_hex.to_string())],
        limit: Some(MAX_INBOUND_NOTES as u32),
        ..Default::default()
    }
    .into_logical_interest(
        InterestId(stable_hash64(&format!("podcast.agent_notes.{my_pubkey_hex}"))),
        InterestScope::Global,
        InterestLifecycle::OneShot,
    )
}

/// Fetch inbound kind:1 notes addressed to the active account via NMP relay
/// pool. Results arrive via [`AgentNotesObserver`].
pub fn handle_fetch_agent_notes(
    app: *mut NmpApp,
    identity: &Arc<Mutex<IdentityStore>>,
) -> serde_json::Value {
    let my_pubkey_hex = match identity.lock() {
        Ok(id) => match id.pubkey_hex.clone() {
            Some(p) => p,
            None => return serde_json::json!({"ok": false, "error": "not signed in"}),
        },
        Err(_) => return serde_json::json!({"ok": false, "error": "identity poisoned"}),
    };
    push_interest_via_nmp(app, agent_notes_interest(&my_pubkey_hex));
    serde_json::json!({"ok": true, "status": "subscribed"})
}

// ── publish ──────────────────────────────────────────────────────────────────

/// Build kind:1 NIP-10 tags for an agent note.
pub(crate) fn build_agent_note_tags(
    recipient_pubkey_hex: &str,
    root_event_id: Option<&str>,
) -> Result<Vec<Vec<String>>, String> {
    nostr::PublicKey::parse(recipient_pubkey_hex)
        .map_err(|e| format!("invalid recipient pubkey: {e}"))?;
    let mut tags: Vec<Vec<String>> = Vec::with_capacity(2);
    if let Some(root) = root_event_id {
        if !root.is_empty() {
            tags.push(vec!["e".to_string(), root.to_string(), String::new(), "root".to_string()]);
        }
    }
    tags.push(vec!["p".to_string(), recipient_pubkey_hex.to_string()]);
    Ok(tags)
}

/// Publish a kind:1 agent note via `nmp.publish { PublishRaw }`.
/// NMP signs with the active user signer — no secret bytes in app code.
pub fn handle_publish_agent_note(
    app: *mut NmpApp,
    identity: &Arc<Mutex<IdentityStore>>,
    recipient_pubkey_hex: &str,
    content: &str,
    root_event_id: Option<&str>,
) -> serde_json::Value {
    if content.trim().is_empty() {
        return serde_json::json!({"ok": false, "error": "empty note"});
    }
    match identity.lock() {
        Ok(id) if id.pubkey_hex.is_none() => {
            return serde_json::json!({"ok": false, "error": "not signed in"});
        }
        Err(_) => return serde_json::json!({"ok": false, "error": "identity poisoned"}),
        _ => {}
    }
    let tags = match build_agent_note_tags(recipient_pubkey_hex, root_event_id) {
        Ok(t) => t,
        Err(e) => return serde_json::json!({"ok": false, "error": e}),
    };
    let status = publish_raw_via_nmp(app, 1, &tags, content);
    serde_json::json!({"ok": true, "status": status})
}

// ── observer ─────────────────────────────────────────────────────────────────

/// Receives inbound kind:1 notes from NMP's relay pool addressed to the
/// active account, filters self-authored events, and writes to the cache.
pub struct AgentNotesObserver {
    identity: Arc<Mutex<IdentityStore>>,
    agent_notes_cache: Arc<Mutex<Vec<AgentNoteSummary>>>,
    rev: Arc<AtomicU64>,
}

impl AgentNotesObserver {
    pub fn new(
        identity: Arc<Mutex<IdentityStore>>,
        agent_notes_cache: Arc<Mutex<Vec<AgentNoteSummary>>>,
        rev: Arc<AtomicU64>,
    ) -> Self {
        Self { identity, agent_notes_cache, rev }
    }
}

impl KernelEventObserver for AgentNotesObserver {
    fn on_kernel_event(&self, event: &KernelEvent) {
        if event.kind != 1 {
            return;
        }
        // Drop self-authored notes.
        let my_pubkey = self.identity.lock().ok()
            .and_then(|id| id.pubkey_hex.clone())
            .unwrap_or_default();
        if event.author == my_pubkey {
            return;
        }

        let author_npub = nostr::PublicKey::parse(&event.author)
            .ok()
            .and_then(|pk| pk.to_bech32().ok())
            .unwrap_or_else(|| event.author.clone());

        let root_event_id = extract_nip10_root(&event.tags);

        let note = AgentNoteSummary {
            id: event.id.clone(),
            author_npub,
            content: event.content.clone(),
            created_at: event.created_at as i64,
            root_event_id,
            trusted: false,
        };

        if let Ok(mut cache) = self.agent_notes_cache.lock() {
            if !cache.iter().any(|n| n.id == note.id) {
                cache.push(note);
                cache.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                if cache.len() > MAX_INBOUND_NOTES {
                    cache.truncate(MAX_INBOUND_NOTES);
                }
                self.rev.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

fn extract_nip10_root(tags: &[Vec<String>]) -> Option<String> {
    let mut first_e: Option<String> = None;
    for tag in tags {
        if tag.first().map(|s| s.as_str()) != Some("e") {
            continue;
        }
        let id = tag.get(1).filter(|s| !s.is_empty()).cloned()?;
        if tag.get(3).map(|s| s.as_str()) == Some("root") {
            return Some(id);
        }
        if first_e.is_none() {
            first_e = Some(id);
        }
    }
    first_e
}

#[cfg(test)]
#[path = "agent_note_handler_tests.rs"]
mod tests;
