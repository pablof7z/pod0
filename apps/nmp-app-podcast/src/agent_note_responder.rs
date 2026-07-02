//! Kernel-owned auto-responder for trusted inbound kind:1 agent notes (D0).
//!
//! ## Restored capability
//!
//! The Swift `NostrAgentResponder.swift` auto-reply loop was deleted in #248
//! (kernel-signing refactor). The trust gate that was the only blocker landed
//! in #419 (`ActiveFollowSet::predicate()` + the trust verdict on `CachedAgentNote`).
//! This module restores autopilot IN THE KERNEL: trusted kind:1 → LLM reply
//! → publish.
//!
//! ## v1 scope
//!
//! This is a deliberate v1 with a minimal surface:
//!
//! * **Trust gate**: only notes with `trusted: true` (i.e. from the active
//!   account's NIP-02 follow set) trigger a reply. Untrusted notes are silently
//!   ignored.
//! * **Dedup**: responded event IDs are persisted via
//!   [`crate::store::agent_note_responder_cache`] so we never reply twice to
//!   the same event, even across restarts.
//! * **Turn cap** (`MAX_OUTGOING_TURNS_PER_ROOT = 10`): once we've published
//!   10 outbound replies on a given root thread, further notes on that root
//!   are suppressed — defence against runaway agent-on-agent loops.
//! * **`wtd-end` gate**: if the inbound note carries a `wtd-end` tag, the
//!   conversation is over and we do NOT reply.
//! * **D6 degrade**: LLM unavailable / no key / no identity → no reply, no
//!   crash. The responder is best-effort; a failed LLM call does NOT poison the
//!   dedup cache so the same event can be retried on the next delivery.
//! * **D8**: the LLM call is spawned off the observer thread via the shared
//!   Tokio runtime; the observer is never blocked.
//!
//! ## v1 exclusions (deferred)
//!
//! * The 20-turn tool loop (podcast-domain tools during auto-reply).
//! * The owner-consult `ask` coordinator for peer-initiated actions.
//! * Outgoing-turn capture into the conversations projection (B's territory).
//!
//! ## Integration
//!
//! Called from [`crate::agent_note_handler::AgentNotesObserver::on_kernel_event`]
//! AFTER the note is written to the cache but ONLY when `trusted: true` (i.e.
//! the note's author hex is in the active account's NIP-02 follow set).
//!
//! The responder does NOT touch snapshot projections, DTOs, or the pub-path
//! outgoing-turn capture — those are B's territory.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use tokio::runtime::Runtime;

use nmp_native_runtime::NmpApp;

use crate::agent_llm::FAST_MODEL;
use crate::agent_note_handler::{handle_publish_agent_note, CachedAgentNote};
use crate::llm::complete_for_role;
use crate::snapshot_signal::SnapshotUpdateSignal;
use crate::state::{Domain, DomainRevs};
use crate::store::agent_note_responder_cache::{save_responder_cache, ResponderCache};
use crate::store::identity::IdentityStore;
use crate::store::outbound_turn_cache::{save_outbound_turn_cache, OutboundTurn, OutboundTurnCache};
use crate::store::PodcastStore;


/// Per-root outgoing turn cap. Once we've published this many outbound replies
/// on the same conversation root, further inbounds on that root are suppressed —
/// defence against runaway agent-on-agent loops. Matches the Swift reference
/// (`maxOutgoingTurnsPerRoot = 10`).
pub(crate) const MAX_OUTGOING_TURNS_PER_ROOT: u32 = 10;

/// NIP-10 / WTD end-conversation tag name. When an inbound note carries this
/// tag, we do NOT invoke the LLM or publish a reply. Matches the Swift
/// reference (`endConversationTagName = "wtd-end"`).
const END_CONVERSATION_TAG: &str = "wtd-end";

/// Check whether the note's tag list contains a `wtd-end` marker.
fn has_end_tag(tags: &[Vec<String>]) -> bool {
    tags.iter()
        .any(|t| t.first().map(|s| s.as_str()) == Some(END_CONVERSATION_TAG))
}

/// System prompt for the responder role. In v1 this is the same base prompt as
/// the agent chat; a richer peer-agent-specific prompt is a v2 follow-up.
fn responder_system_prompt(store: Option<&Arc<Mutex<PodcastStore>>>) -> String {
    // Reuse the memory-augmented prompt so the responder has user context.
    crate::agent_llm::build_system_prompt_with_memory(store)
}

/// Entry point called from `AgentNotesObserver::on_kernel_event` when a
/// TRUSTED kind:1 note lands.
///
/// Fires off the async responder task onto the shared runtime; returns
/// immediately (non-blocking, D8).
///
/// # Arguments
/// * `note` – the inbound note (already cached; trust has been verified by caller).
/// * `app`  – live `*mut NmpApp` pointer for publish via NMP relay pool.
/// * `identity` – shared identity store (read-only; used for signing identity).
/// * `store` – shared podcast store (read-only; used for LLM model config).
/// * `responder_cache` – shared in-memory responder state (dedup + turn counts).
/// * `outbound_turn_cache` – disk-persistence cache for outbound turns (`None` in tests).
/// * `social_outbound_slot` – in-memory projection slot for immediate UI update (`None` in tests).
/// * `runtime` – shared Tokio runtime (spawn target).
/// * `signal` – optional snapshot signal (bumped after recording an outbound turn).
#[allow(clippy::too_many_arguments)]
pub(crate) fn maybe_respond_to_note(
    note: CachedAgentNote,
    app: *mut NmpApp,
    identity: Arc<Mutex<IdentityStore>>,
    store: Arc<Mutex<PodcastStore>>,
    responder_cache: Arc<Mutex<ResponderCache>>,
    outbound_turn_cache: Option<Arc<Mutex<OutboundTurnCache>>>,
    social_outbound_slot: Option<Arc<Mutex<Vec<OutboundTurn>>>>,
    runtime: &Arc<Runtime>,
    signal: Option<SnapshotUpdateSignal>,
    domain_revs: Option<Arc<DomainRevs>>,
) {
    // ── Guard 1: dedup ────────────────────────────────────────────────────────
    // Check in the caller thread (cheap, avoids spawning a task for a known dup).
    if let Ok(cache) = responder_cache.lock() {
        if cache.already_responded(&note.id) {
            return;
        }
    }

    // ── Guard 2: wtd-end tag ──────────────────────────────────────────────────
    // `CachedAgentNote` doesn't store raw tags, only the root_event_id.
    // The observer passes tags separately — but we can't add fields without
    // breaking the type. So we check end-of-conversation via the inbound
    // note's tags list supplied separately. For now the tag check is done in
    // the observer before calling us; this comment documents the contract.
    // (The observer filters wtd-end in `try_respond_to_trusted_note` below.)

    // ── Guard 3: turn cap ─────────────────────────────────────────────────────
    let root_id = note
        .root_event_id
        .clone()
        .unwrap_or_else(|| note.id.clone());
    if let Ok(cache) = responder_cache.lock() {
        if cache.turns_for_root(&root_id) >= MAX_OUTGOING_TURNS_PER_ROOT {
            return;
        }
    }

    // ── Spawn async task ──────────────────────────────────────────────────────
    // Wrap the raw pointer in `SendApp` so the async block can cross thread
    // boundaries (the `*mut NmpApp` is only read-accessed inside the task;
    // see the `SendApp` SAFETY comment above).
    let note_id = note.id.clone();
    let note_content = note.content.clone();
    let author_hex = note.author_hex.clone();
    let inbound_id = note.id.clone();
    let root_id_for_task = root_id.clone();
    // Cast the raw pointer to `usize` so the async closure is `Send`.
    // The pointer is reconstructed inside `respond_async` after all await points.
    // SAFETY: `usize` is the right integer width for a pointer on every target
    // platform this crate builds for (x86-64 / arm64 / aarch64-android).
    let app_addr = app as usize;

    runtime.spawn(async move {
        respond_async(
            note_id,
            note_content,
            author_hex,
            inbound_id,
            root_id_for_task,
            app_addr,
            identity,
            store,
            responder_cache,
            outbound_turn_cache,
            social_outbound_slot,
            signal,
            domain_revs,
        )
        .await;
    });
}

/// Async body: build LLM reply → publish → persist dedup state + record outbound turn.
///
/// D6: any failure along the path (lock poisoning, missing identity, LLM
/// error, publish failure) is logged and silently dropped. The dedup cache is
/// updated ONLY on a successful publish so a failed attempt can be retried on
/// relay re-delivery.
#[allow(clippy::too_many_arguments)]
async fn respond_async(
    event_id: String,
    inbound_content: String,
    author_hex: String,
    inbound_event_id: String,
    root_event_id: String,
    app_addr: usize,
    identity: Arc<Mutex<IdentityStore>>,
    store: Arc<Mutex<PodcastStore>>,
    responder_cache: Arc<Mutex<ResponderCache>>,
    outbound_turn_cache: Option<Arc<Mutex<OutboundTurnCache>>>,
    social_outbound_slot: Option<Arc<Mutex<Vec<OutboundTurn>>>>,
    signal: Option<SnapshotUpdateSignal>,
    domain_revs: Option<Arc<DomainRevs>>,
) {
    // ── Re-check dedup inside the task (race window guard) ───────────────────
    if let Ok(cache) = responder_cache.lock() {
        if cache.already_responded(&event_id) {
            return;
        }
    }

    // ── Build LLM reply ──────────────────────────────────────────────────────
    let system = responder_system_prompt(Some(&store));

    // Read the configured responder role model. The "agent initial" model is
    // the closest existing role to a peer-responder — it's the model the user
    // picks for interactive chat, which is exactly the right register here.
    let role_cfg = store
        .lock()
        .ok()
        .map(|s| s.agent_initial_model().to_owned())
        .unwrap_or_default();

    let reply = match complete_for_role(
        &store,
        &role_cfg,
        FAST_MODEL,
        &system,
        &inbound_content,
    )
    .await
    {
        Ok(r) if !r.trim().is_empty() => r,
        Ok(_) => {
            // Empty model response — skip publish (D6 degrade).
            eprintln!(
                "[agent_note_responder] LLM returned empty response for event {event_id}"
            );
            return;
        }
        Err(e) => {
            // LLM unavailable / no key — silently drop, D6.
            eprintln!(
                "[agent_note_responder] LLM error for event {event_id}: {e}"
            );
            return;
        }
    };

    // ── Publish via existing NIP-10 path ─────────────────────────────────────
    // Reconstruct the raw pointer AFTER all await points — no non-Send value
    // is held across any `.await` boundary.  `usize` is Send; `*mut NmpApp`
    // is not, so we only hold it in the synchronous publish call below.
    // `handle_publish_agent_note` builds the correct NIP-10 e/p tags and
    // routes through NMP's relay pool so no signing logic lives here.
    let app = app_addr as *mut NmpApp;
    let publish_result = handle_publish_agent_note(
        app,
        &identity,
        &author_hex,
        &reply,
        Some(&root_event_id),
        Some(&inbound_event_id),
        &[], // no NIP-72 channel anchors for peer-to-peer replies
    );

    if publish_result["ok"].as_bool() != Some(true) {
        let err = publish_result["error"]
            .as_str()
            .unwrap_or("unknown publish error");
        eprintln!(
            "[agent_note_responder] publish failed for event {event_id}: {err}"
        );
        return;
    }

    // ── Record response + persist ─────────────────────────────────────────────
    // Only update the dedup cache after a successful publish. This ensures that
    // a LLM error or publish failure allows relay re-delivery to retry.
    let data_dir = store
        .lock()
        .ok()
        .and_then(|s| s.data_dir().map(|p| p.to_path_buf()));

    if let Ok(mut cache) = responder_cache.lock() {
        cache.record_response(&event_id, &root_event_id);
        if let Some(dir) = &data_dir {
            // D6: persistence failure is non-fatal; in-memory cache stays authoritative.
            if let Err(e) = save_responder_cache(dir, &cache) {
                eprintln!(
                    "[agent_note_responder] failed to persist responder cache: {e}"
                );
            }
        }
    }

    // ── Capture outbound turn ─────────────────────────────────────────────────
    // Extract the published event id from the publish result. NMP returns the
    // signed event id in publish_result["event_id"] when ok=true; fall back to
    // a synthetic id derived from the inbound event if absent (D6 graceful).
    let published_event_id = publish_result
        .get("event_id")
        .and_then(|v| v.as_str())
        .unwrap_or(&event_id)
        .to_string();

    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let outbound_turn = OutboundTurn {
        event_id: published_event_id,
        root_event_id: root_event_id.clone(),
        counterparty_hex: author_hex.clone(),
        content: reply.clone(),
        created_at: now_secs,
    };

    // Update in-memory projection slot first (D8: non-blocking slot write).
    if let Some(slot) = &social_outbound_slot {
        if let Ok(mut turns) = slot.lock() {
            turns.push(outbound_turn.clone());
        }
    }

    // Update disk-persistence cache + persist.
    if let Some(cache_arc) = &outbound_turn_cache {
        if let Ok(mut cache) = cache_arc.lock() {
            cache.record(outbound_turn);
            if let Some(dir) = &data_dir {
                if let Err(e) = save_outbound_turn_cache(dir, &cache) {
                    eprintln!(
                        "[agent_note_responder] failed to persist outbound turn cache: {e}"
                    );
                }
            }
        }
    }

    // Bump the snapshot so the conversation view refreshes (D8 — bump after
    // releasing all slot locks to avoid priority inversion).
    //
    // This MUST advance `domain_revs.social` (not just the global rev) or the
    // `podcast.social` push sidecar — which gates on the social domain rev —
    // never re-emits and the outbound turn never reaches iOS/Android. We mirror
    // the exact two-step `Infra::bump()` performs (domain rev fetch_add, then
    // the global signal post) because the off-actor task holds a raw
    // `Arc<DomainRevs>` rather than a full scoped `Infra`.
    if let Some(dr) = &domain_revs {
        dr.counter(Domain::Social).fetch_add(1, Ordering::Relaxed);
    }
    if let Some(s) = &signal {
        s.bump();
    }

    eprintln!(
        "[agent_note_responder] replied to event {event_id} from {author_hex}"
    );
}

/// Thin shell called from `AgentNotesObserver::on_kernel_event` — validates
/// the trust flag, wtd-end gate, and dispatches to `maybe_respond_to_note`.
///
/// `trusted`: pass the live `ActiveFollowSet::predicate()(&note.author_hex)`.
/// `tags`: the raw tag list from the `KernelEvent` for `wtd-end` inspection.
#[allow(clippy::too_many_arguments)]
pub(crate) fn try_respond_to_trusted_note(
    note: &CachedAgentNote,
    tags: &[Vec<String>],
    trusted: bool,
    app: *mut NmpApp,
    identity: Arc<Mutex<IdentityStore>>,
    store: Arc<Mutex<PodcastStore>>,
    responder_cache: Arc<Mutex<ResponderCache>>,
    outbound_turn_cache: Option<Arc<Mutex<OutboundTurnCache>>>,
    social_outbound_slot: Option<Arc<Mutex<Vec<OutboundTurn>>>>,
    runtime: &Arc<Runtime>,
    signal: Option<SnapshotUpdateSignal>,
    domain_revs: Option<Arc<DomainRevs>>,
) {
    // Gate 1: trust (only respond to followed peers).
    if !trusted {
        return;
    }
    // Gate 2: end-conversation tag.
    if has_end_tag(tags) {
        return;
    }
    maybe_respond_to_note(
        note.clone(),
        app,
        identity,
        store,
        responder_cache,
        outbound_turn_cache,
        social_outbound_slot,
        runtime,
        signal,
        domain_revs,
    );
}

#[cfg(test)]
#[path = "agent_note_responder_tests.rs"]
mod tests;
