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
//! ## Trust gate — computed live at projection, NOT frozen at receipt
//!
//! The conversation trust field (`NostrConversationDTO::trusted`) reflects
//! `ActiveFollowSet::predicate()` membership at **projection-build time**, not at
//! the moment the kind:1 note was received.
//!
//! The observer caches the raw note as a [`CachedAgentNote`] that retains the
//! author **hex** pubkey but carries **no** `trusted` stamp. The trust verdict is
//! recomputed every time the snapshot is built, inside
//! [`crate::state::social::SocialState::nostr_conversations_snapshot`], by applying the
//! shared live `ActiveFollowSet` predicate to each cached note's author hex.
//!
//! This is the load-bearing correctness property: a note from X received *before*
//! the user follows X starts untrusted, and flips to `trusted: true` on the very
//! next projection after the follow lands — and back to `false` on unfollow.
//! Stamping at receipt (the old design) would freeze a stale verdict because the
//! cache dedups by event id and never refreshes the row.
//!
//! Because the verdict is recomputed at build time, the observer-registration
//! order relative to `ActiveFollowSet` no longer matters for correctness.
//!
//! ## LLM responder loop
//!
//! Restored in `agent_note_responder.rs` (PR #420 / feat/kernel-kind1-auto-responder).
//! When a trusted kind:1 note arrives, the observer calls
//! [`crate::agent_note_responder::try_respond_to_trusted_note`], which spawns an
//! async LLM-reply task off the actor thread (D8). The trust verdict is computed
//! live against the shared `ActiveFollowSet` before dispatch.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use nostr::nips::nip19::ToBech32;

use nmp_planner::interest::{InterestId, InterestLifecycle, InterestScope, LogicalInterest};
use nmp_planner::stable_hash::stable_hash64;
use nmp_core::substrate::{KernelEvent, ViewDependencies};
use nmp_core::KernelEventObserver;

use crate::nmp_dispatch::{publish_raw_via_nmp, push_interest_via_nmp};
use crate::snapshot_signal::SnapshotUpdateSignal;
use crate::state::Infra;
use crate::store::agent_note_responder_cache::ResponderCache;
use crate::store::approved_peer_store::ApprovedPeerStore;
use crate::store::identity::IdentityStore;
use crate::store::outbound_turn_cache::OutboundTurnCache;
use crate::store::PodcastStore;
use nmp_ffi::NmpApp;
use nmp_nip02::ActiveFollowSet;
use tokio::runtime::Runtime;

const MAX_INBOUND_NOTES: usize = 200;

/// Internal cached representation of an inbound kind:1 agent note.
///
/// Distinct from the wire DTO (the flat per-note projection was retired;
/// `NostrConversationDTO` turns carry trust now):
/// it retains the author **hex** pubkey so the social projection can recompute
/// the trust verdict live (against the `ActiveFollowSet`) at every snapshot
/// build, and it carries **no** `trusted` field — the verdict is never frozen
/// at receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CachedAgentNote {
    /// Event id (lowercase hex) — stable identifier, used for dedup.
    pub id: String,
    /// Author hex pubkey (64 chars) — the trust-gate input. Retained so the
    /// projection can apply the live `ActiveFollowSet` predicate.
    pub author_hex: String,
    /// Author bech32 (`npub1…`) — pre-encoded for the iOS truncated-key
    /// fallback.
    pub author_npub: String,
    /// Note body — the raw `content` field of the kind:1 event.
    pub content: String,
    /// Unix seconds (matches NIP-01 `created_at`).
    pub created_at: i64,
    /// NIP-10 conversation root event id (lowercase hex) when the note is a
    /// reply, else `None`.
    pub root_event_id: Option<String>,
}

// ── subscribe helpers ────────────────────────────────────────────────────────

fn agent_notes_interest(my_pubkey_hex: &str) -> LogicalInterest {
    ViewDependencies {
        kinds: vec![1],
        tag_refs: vec![("p".to_string(), my_pubkey_hex.to_string())],
        limit: Some(MAX_INBOUND_NOTES as u32),
        ..Default::default()
    }
    .into_logical_interest(
        InterestId(stable_hash64(&format!(
            "podcast.agent_notes.{my_pubkey_hex}"
        ))),
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
/// Build NIP-10 tags for a kind:1 agent note from semantic values.
/// Rust owns all tag construction — Swift passes only data, never arrays.
pub(crate) fn build_agent_note_tags(
    recipient_pubkey_hex: &str,
    root_event_id: Option<&str>,
    inbound_event_id: Option<&str>,
    root_a_tags: &[String],
) -> Result<Vec<Vec<String>>, String> {
    nostr::PublicKey::parse(recipient_pubkey_hex)
        .map_err(|e| format!("invalid recipient pubkey: {e}"))?;
    let mut tags: Vec<Vec<String>> = Vec::new();
    // NIP-72 channel anchors first.
    for coord in root_a_tags {
        if !coord.is_empty() {
            tags.push(vec!["a".to_string(), coord.clone()]);
        }
    }
    // NIP-10 root marker.
    if let Some(root) = root_event_id.filter(|s| !s.is_empty()) {
        tags.push(vec![
            "e".to_string(),
            root.to_string(),
            String::new(),
            "root".to_string(),
        ]);
    }
    // NIP-10 reply marker (only when different from root).
    if let Some(inbound) = inbound_event_id.filter(|s| !s.is_empty()) {
        let is_new = root_event_id.map_or(true, |r| r != inbound);
        if is_new {
            tags.push(vec![
                "e".to_string(),
                inbound.to_string(),
                String::new(),
                "reply".to_string(),
            ]);
        }
    }
    // Recipient.
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
    inbound_event_id: Option<&str>,
    root_a_tags: &[String],
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
    let tags = match build_agent_note_tags(
        recipient_pubkey_hex,
        root_event_id,
        inbound_event_id,
        root_a_tags,
    ) {
        Ok(t) => t,
        Err(e) => return serde_json::json!({"ok": false, "error": e}),
    };
    let status = publish_raw_via_nmp(app, 1, &tags, content);
    serde_json::json!({"ok": true, "status": status})
}

// ── observer ─────────────────────────────────────────────────────────────────

/// Receives inbound kind:1 notes from NMP's relay pool addressed to the
/// active account, filters self-authored events, and writes the raw note
/// (author hex retained, NO trust stamp) to the cache.
///
/// The trust verdict is computed later, at projection-build time, in
/// [`crate::state::social::SocialState::nostr_conversations_snapshot`] — see the
/// module-level "Trust gate" doc. The observer never freezes `trusted`.
///
/// When a note is TRUSTED (its author hex is in the active account's NIP-02
/// follow set at reception time), the observer calls
/// [`crate::agent_note_responder::try_respond_to_trusted_note`] to spawn an
/// async LLM-reply task off the actor thread (D8).
pub struct AgentNotesObserver {
    identity: Arc<Mutex<IdentityStore>>,
    agent_notes_cache: Arc<Mutex<Vec<CachedAgentNote>>>,
    rev: Arc<AtomicU64>,
    snapshot_signal: Option<SnapshotUpdateSignal>,
    /// Shared live follow set for trust-gate checks at reception time.
    /// `None` in test / legacy paths → all notes treated as untrusted.
    follow_set: Option<Arc<ActiveFollowSet>>,
    /// Shared kernel-owned approve/block store for the composed trust gate.
    /// `None` in test / legacy paths → only follow-set trust applies.
    approved_peer_store: Option<Arc<Mutex<ApprovedPeerStore>>>,
    /// Shared podcast store (LLM model config for the responder).
    /// `None` in test paths → responder is disabled.
    store: Option<Arc<Mutex<PodcastStore>>>,
    /// Shared responder cache (dedup + turn counts).
    /// `None` in test / legacy paths → responder is disabled.
    responder_cache: Option<Arc<Mutex<ResponderCache>>>,
    /// Outbound-turn cache for disk persistence of published auto-reply turns.
    /// `None` in test / legacy paths → outbound capture is disabled.
    outbound_turn_cache: Option<Arc<Mutex<OutboundTurnCache>>>,
    /// Shared outbound-turns slot (Vec from SocialState.outbound_turns).
    /// Updated in-memory so the conversation projection sees new outbound turns
    /// immediately without waiting for a full reload. `None` in test paths.
    social_outbound_slot: Option<Arc<Mutex<Vec<crate::store::outbound_turn_cache::OutboundTurn>>>>,
    /// Shared Tokio runtime for spawning responder tasks.
    /// `None` in test / legacy paths → responder is disabled.
    runtime: Option<Arc<Runtime>>,
    /// `Domain::Social`-scoped `Infra` clone (from `SocialState.infra`).
    ///
    /// `infra.bump()` advances BOTH `domain_revs.social` (so the
    /// `podcast.social` push sidecar re-emits) AND the global rev/signal (so a
    /// tick fires at all). This is the SAME canonical mutation-site idiom every
    /// working reactive domain uses; the bare `snapshot_signal.bump()` only
    /// touches the global rev and would leave `domain_revs.social` frozen at 1,
    /// making the social sidecar emit once then idle forever.
    /// `None` in test / legacy paths (those fall back to the global rev).
    social_infra: Option<Infra>,
    /// Raw NmpApp pointer for the responder's publish call.
    /// Null in test paths → responder is disabled.
    app: *mut NmpApp,
}

// SAFETY: `app` is a raw pointer that is only READ (passed to
// `handle_publish_agent_note`, which null-checks it). The observer is only
// registered once and lives for the app's lifetime; `nmp_app_free` joins the
// actor thread before freeing the pointer.
unsafe impl Send for AgentNotesObserver {}
unsafe impl Sync for AgentNotesObserver {}

impl AgentNotesObserver {
    pub fn new(
        identity: Arc<Mutex<IdentityStore>>,
        agent_notes_cache: Arc<Mutex<Vec<CachedAgentNote>>>,
        rev: Arc<AtomicU64>,
    ) -> Self {
        Self {
            identity,
            agent_notes_cache,
            rev,
            snapshot_signal: None,
            follow_set: None,
            approved_peer_store: None,
            store: None,
            responder_cache: None,
            outbound_turn_cache: None,
            social_outbound_slot: None,
            runtime: None,
            social_infra: None,
            app: std::ptr::null_mut(),
        }
    }

    pub(crate) fn with_snapshot_signal(mut self, snapshot_signal: SnapshotUpdateSignal) -> Self {
        self.snapshot_signal = Some(snapshot_signal);
        self
    }

    /// Wire the `Domain::Social`-scoped `Infra` so inbound-note mutations bump
    /// `domain_revs.social` (driving the `podcast.social` sidecar re-emit) in
    /// addition to the global rev. Pass `SocialState.infra.clone()`.
    pub(crate) fn with_social_infra(mut self, infra: Infra) -> Self {
        self.social_infra = Some(infra);
        self
    }

    /// Bump the snapshot after an inbound-note cache mutation.
    ///
    /// Prefers the `Domain::Social`-scoped `Infra` (production): `infra.bump()`
    /// advances `domain_revs.social` AND the global rev/signal — the canonical
    /// reactive-domain mutation idiom. Falls back to the bare global signal,
    /// then to a raw `rev` increment (legacy/test paths with no social infra).
    fn bump_social(&self) {
        if let Some(infra) = &self.social_infra {
            infra.bump();
        } else if let Some(signal) = &self.snapshot_signal {
            signal.bump();
        } else {
            self.rev.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Wire the responder: enable auto-reply for trusted notes.
    ///
    /// All arguments are `Arc` clones from `register.rs`; the `app`
    /// pointer is the live `*mut NmpApp` that `handle_publish_agent_note` uses.
    ///
    /// `outbound_turn_cache` + `social_outbound_slot` enable outbound-turn
    /// capture: after a successful publish the responder records the turn for
    /// disk persistence AND updates the in-memory projection slot so the
    /// conversation view reflects the outbound reply immediately.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn with_responder(
        mut self,
        app: *mut NmpApp,
        follow_set: Arc<ActiveFollowSet>,
        approved_peer_store: Arc<Mutex<ApprovedPeerStore>>,
        store: Arc<Mutex<PodcastStore>>,
        responder_cache: Arc<Mutex<ResponderCache>>,
        outbound_turn_cache: Arc<Mutex<OutboundTurnCache>>,
        social_outbound_slot: Arc<Mutex<Vec<crate::store::outbound_turn_cache::OutboundTurn>>>,
        runtime: Arc<Runtime>,
    ) -> Self {
        self.app = app;
        self.follow_set = Some(follow_set);
        self.approved_peer_store = Some(approved_peer_store);
        self.store = Some(store);
        self.responder_cache = Some(responder_cache);
        self.outbound_turn_cache = Some(outbound_turn_cache);
        self.social_outbound_slot = Some(social_outbound_slot);
        self.runtime = Some(runtime);
        self
    }
}

impl KernelEventObserver for AgentNotesObserver {
    fn on_kernel_event(&self, event: &KernelEvent) {
        if event.kind != 1 {
            return;
        }
        // Drop self-authored notes.
        let my_pubkey = self
            .identity
            .lock()
            .ok()
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

        // Store the raw note with author hex retained. The trust verdict is
        // NOT computed here — it is recomputed live at projection-build time
        // against the shared `ActiveFollowSet` (see SocialState).
        let note = CachedAgentNote {
            id: event.id.clone(),
            author_hex: event.author.clone(),
            author_npub,
            content: event.content.clone(),
            created_at: event.created_at as i64,
            root_event_id,
        };

        let is_new = if let Ok(mut cache) = self.agent_notes_cache.lock() {
            if !cache.iter().any(|n| n.id == note.id) {
                cache.push(note.clone());
                cache.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                if cache.len() > MAX_INBOUND_NOTES {
                    cache.truncate(MAX_INBOUND_NOTES);
                }
                true
            } else {
                false
            }
        } else {
            false
        };

        // Bump AFTER releasing the cache lock (D8: never hold a `Slot`/`Mutex`
        // guard across `bump()` — it posts on the actor channel). `bump_social`
        // advances `domain_revs.social` so the `podcast.social` sidecar
        // re-emits on the next tick (not just the global rev).
        if is_new {
            self.bump_social();
        }

        // ── Auto-responder hook ──────────────────────────────────────────────
        // Fire ONLY for new, trusted notes (avoid double-reply on relay re-delivery).
        // Trust uses the COMPOSED predicate: (follow || approve) && !block, same
        // logic as `SocialState::trust_predicate()`. The projection ALSO recomputes
        // it at build time (two separate consumers, same predicate contract).
        if is_new {
            if let (Some(fs), Some(store), Some(rc), Some(rt)) = (
                &self.follow_set,
                &self.store,
                &self.responder_cache,
                &self.runtime,
            ) {
                // Build the composed trust predicate for this reception event.
                //
                // POISONED-LOCK FAIL-CLOSED: if the approved-store mutex is
                // poisoned we cannot read the blocklist. Treating both sets as
                // empty would be fail-OPEN — a blocked-but-followed peer would
                // get an auto-reply, violating "block is absolute". Instead we
                // deny (trusted=false) so a poisoned lock never auto-responds.
                let trusted = {
                    let follow_pred = fs.predicate();
                    let mut fail_closed = false;
                    let (approved_snap, blocked_snap) = if let Some(ref arc) = self.approved_peer_store {
                        if let Ok(s) = arc.lock() {
                            (s.approved.clone(), s.blocked.clone())
                        } else {
                            fail_closed = true;
                            (Default::default(), Default::default())
                        }
                    } else {
                        (Default::default(), Default::default())
                    };
                    let hex = note.author_hex.as_str();
                    !fail_closed
                        && !blocked_snap.contains(hex)
                        && (follow_pred(hex) || approved_snap.contains(hex))
                };
                crate::agent_note_responder::try_respond_to_trusted_note(
                    &note,
                    &event.tags,
                    trusted,
                    self.app,
                    self.identity.clone(),
                    store.clone(),
                    rc.clone(),
                    self.outbound_turn_cache.clone(),
                    self.social_outbound_slot.clone(),
                    rt,
                    self.snapshot_signal.clone(),
                    // `domain_revs` from the Domain::Social-scoped infra so the
                    // outbound-turn bump advances `domain_revs.social`, driving
                    // the `podcast.social` sidecar re-emit (not just global rev).
                    self.social_infra.as_ref().map(|i| i.domain_revs.clone()),
                );
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
