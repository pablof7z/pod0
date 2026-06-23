//! Reactive NIP-02 social-graph handler.
//!
//! ## Design (post-reactive migration)
//!
//! The old implementation used a one-shot 8-second-timeout relay pull via a
//! bespoke `tokio::spawn` + `subscribe_until_eose` loop.  That violates the
//! D8 push-seam doctrine (no polling, no hardcoded relay URLs in app code).
//!
//! The new design is observer-only:
//!
//! * [`FollowListObserver`] wraps the upstream [`nmp_nip02::FollowListProjection`]
//!   (a `KernelEventObserver` for kind:3).  It updates the shared
//!   `social_slot` (`Option<SocialSnapshot>`) on every kind:3 push frame and
//!   bumps the snapshot signal so the iOS shell gets an immediate push.
//!
//! * The `account_profile_interest` standing subscription that the kernel
//!   already opens for kind:0 + kind:3 + kind:10002 delivers kind:3 events
//!   to every registered `KernelEventObserver` without any extra subscription
//!   request.  No `EnsureInterest` call, no manual relay URL, no polling.
//!
//! * [`handle_fetch_contacts`] is kept as a refresh TRIGGER: Swift can call
//!   `podcast.FetchContacts` to bump the snapshot rev and signal the iOS
//!   shell to re-render even if no new kind:3 has arrived (e.g. on tab focus).
//!   It does NOT open a relay connection itself.
//!
//! ## Trust gate (lives in the social PROJECTION, not here)
//!
//! The trust verdict for inbound agent notes is computed **live at projection
//! time** in [`crate::state::social::SocialState::nostr_conversations_snapshot`],
//! by applying the shared [`nmp_nip02::ActiveFollowSet`] predicate to each
//! conversation's counterparty hex.  This observer module only materialises the follow-list
//! snapshot; it does NOT stamp `trusted`.  See `agent_note_handler.rs` for why
//! the verdict must be recomputed at projection (follow/unfollow must flip the
//! verdict on every existing note, with no stale freeze).
//!
//! ## Domain-scoped bump doctrine (mirrors `AgentNotesObserver`)
//!
//! [`FollowListObserver`] writes to the `podcast.social` sidecar slot
//! (`social_slot`).  Bumping only the bare global signal leaves
//! `domain_revs.social` frozen at 1, making the `podcast.social` push sidecar
//! emit once then idle forever — the same bug that was explicitly fixed in
//! `AgentNotesObserver` (see `agent_note_handler.rs:250-316`).
//!
//! The fix is identical: inject the `Domain::Social`-scoped [`crate::state::Infra`]
//! via [`FollowListObserver::with_social_infra`] and call `infra.bump()` in
//! [`FollowListObserver::bump_social`].  `infra.bump()` advances BOTH
//! `domain_revs.social` AND the global rev/signal — the canonical mutation idiom.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use nostr::nips::nip19::ToBech32;
use nmp_core::substrate::{ContactsLookup, KernelEvent};
use nmp_core::KernelEventObserver;
use nmp_nip02::FollowListProjection;

use crate::ffi::projections::{ContactSummary, SocialSnapshot};
use crate::snapshot_signal::SnapshotUpdateSignal;
use crate::state::Infra;

// ── reactive observer ────────────────────────────────────────────────────────

/// Wraps [`FollowListProjection`] and materialises a [`SocialSnapshot`] on
/// every kind:3 push frame, writing it to the shared `social_slot`.
///
/// Registered as a `KernelEventObserver` via `register.rs`.  The kernel's
/// standing `account_profile_interest` subscription delivers kind:3 events
/// without any extra subscription request — no polling, no hardcoded relay
/// URLs.
pub struct FollowListObserver {
    projection: FollowListProjection,
    social_slot: Arc<Mutex<Option<SocialSnapshot>>>,
    rev: Arc<AtomicU64>,
    snapshot_signal: Option<SnapshotUpdateSignal>,
    /// `Domain::Social`-scoped `Infra` clone (from `SocialState.infra`).
    ///
    /// `infra.bump()` advances BOTH `domain_revs.social` (so the
    /// `podcast.social` push sidecar re-emits) AND the global rev/signal (so a
    /// tick fires at all).  This is the canonical mutation-site idiom every
    /// working reactive domain uses; the bare `snapshot_signal.bump()` only
    /// touches the global rev and would leave `domain_revs.social` frozen at 1,
    /// making the social sidecar emit once then idle forever.
    /// `None` in test / legacy paths (those fall back to the global signal/rev).
    social_infra: Option<Infra>,
}

impl FollowListObserver {
    /// Construct the observer.
    ///
    /// * `active_pubkey` — the kernel's shared active-account slot
    ///   (`NmpApp::active_account_handle()`).
    /// * `contacts_lookup` — the shared `ContactsLookup` the `Kind3Parser`
    ///   writes to; `FollowListProjection` is a thin read-model over it.
    /// * `social_slot` — the shared slot written by this observer and read by
    ///   the snapshot projection.
    /// * `rev` — shared rev counter; bumped on every kind:3 event when no
    ///   `snapshot_signal` is present.
    pub fn new(
        active_pubkey: Arc<Mutex<Option<String>>>,
        contacts_lookup: Arc<dyn ContactsLookup>,
        social_slot: Arc<Mutex<Option<SocialSnapshot>>>,
        rev: Arc<AtomicU64>,
    ) -> Self {
        Self {
            projection: FollowListProjection::new(active_pubkey, contacts_lookup),
            social_slot,
            rev,
            snapshot_signal: None,
            social_infra: None,
        }
    }

    /// Attach a `SnapshotUpdateSignal` so the observer can push frames to the
    /// iOS shell immediately without waiting for unrelated actor traffic.
    pub fn with_snapshot_signal(mut self, signal: SnapshotUpdateSignal) -> Self {
        self.snapshot_signal = Some(signal);
        self
    }

    /// Wire the `Domain::Social`-scoped `Infra` so follow-list mutations bump
    /// `domain_revs.social` (driving the `podcast.social` sidecar re-emit) in
    /// addition to the global rev.  Pass `SocialState.infra.clone()`.
    ///
    /// Mirrors [`crate::agent_note_handler::AgentNotesObserver::with_social_infra`].
    pub fn with_social_infra(mut self, infra: Infra) -> Self {
        self.social_infra = Some(infra);
        self
    }

    /// Bump the snapshot after a follow-list mutation.
    ///
    /// Prefers the `Domain::Social`-scoped `Infra` (production): `infra.bump()`
    /// advances `domain_revs.social` AND the global rev/signal — the canonical
    /// reactive-domain mutation idiom.  Falls back to the bare global signal,
    /// then to a raw `rev` increment (legacy/test paths with no social infra).
    ///
    /// Mirrors [`crate::agent_note_handler::AgentNotesObserver::bump_social`].
    fn bump_social(&self) {
        if let Some(infra) = &self.social_infra {
            infra.bump();
        } else if let Some(signal) = &self.snapshot_signal {
            signal.bump();
        } else {
            self.rev.fetch_add(1, Ordering::Relaxed);
        }
    }
}

impl KernelEventObserver for FollowListObserver {
    /// Forward the event to the inner [`FollowListProjection`], then — if the
    /// event was accepted (kind:3 for the active account) — materialise and
    /// store a fresh [`SocialSnapshot`] and signal the shell.
    ///
    /// Non-kind:3 events return immediately without touching the slot (D8:
    /// bounded, non-blocking work on the actor thread).
    fn on_kernel_event(&self, event: &KernelEvent) {
        if event.kind != 3 {
            return;
        }

        // Read the follow list from the shared ContactsLookup (written by
        // Kind3Parser before this observer fires). FollowListProjection is
        // now a thin read-model over the ContactsLookup — no secondary map.
        let snap = self.projection.snapshot();

        // Materialise ContactSummary rows with bech32 npubs + raw hex.
        // The inner FollowListProjection stores raw hex pubkeys (aim.md §2 —
        // presentation in the app layer).  We bech32-encode here since
        // ContactSummary is the typed shell DTO.  pubkey_hex carries the
        // raw hex so Android can call bridge.claimProfile(pubkeyHex) to
        // trigger kind:0 resolution via the resolved_profiles seam.
        let contacts: Vec<ContactSummary> = snap
            .follows
            .iter()
            .map(|entry| {
                // entry.pubkey is already lowercase hex — clone for pubkey_hex.
                let pubkey_hex = entry.pubkey.clone();
                let npub = nostr::PublicKey::parse(&entry.pubkey)
                    .ok()
                    .and_then(|pk| pk.to_bech32().ok())
                    .unwrap_or_else(|| entry.pubkey.clone());
                ContactSummary {
                    npub,
                    pubkey_hex,
                    display_name: None,
                    picture_url: None,
                }
            })
            .collect();

        let following_count = contacts.len();

        if let Ok(mut slot) = self.social_slot.lock() {
            *slot = Some(SocialSnapshot {
                following: contacts,
                following_count,
                approved_pubkeys: Vec::new(),
                blocked_pubkeys: Vec::new(),
            });
        }

        // Bump `domain_revs.social` AND the global rev/signal via the
        // Domain::Social-scoped Infra (production) — or fall back to the bare
        // global signal / raw rev increment (test/legacy).  This matches the
        // established doctrine from `AgentNotesObserver::bump_social`.
        self.bump_social();
    }
}

// ── refresh trigger ──────────────────────────────────────────────────────────

/// Lightweight refresh trigger for `podcast.FetchContacts`.
///
/// The reactive `FollowListObserver` populates `social_slot` automatically
/// whenever a kind:3 event arrives via the kernel's standing subscription.
/// This function is kept so Swift can explicitly request a snapshot bump (e.g.
/// on Social-tab focus) without duplicating an expensive relay pull.
///
/// It reads the current `social_slot` and, if already populated, bumps the
/// `Domain::Social`-scoped `Infra` (driving the sidecar re-emit AND the global
/// rev); if not yet populated, returns `{"ok":true,"status":"pending"}` — the
/// observer will deliver when kind:3 arrives.
///
/// `social_infra` is the `Domain::Social`-scoped [`Infra`] from
/// `SocialState.infra`.  When `None` (test/legacy paths) the function falls
/// back to the bare `snapshot_signal` / `rev` parameters.
pub fn handle_fetch_contacts(
    social: Arc<Mutex<Option<SocialSnapshot>>>,
    social_infra: Option<&Infra>,
    rev: Arc<AtomicU64>,
    snapshot_signal: Option<&SnapshotUpdateSignal>,
) -> serde_json::Value {
    let has_data = social.lock().ok().and_then(|s| s.clone()).is_some();
    if has_data {
        // Already populated — bump so the shell re-renders the existing data.
        // Prefer the Domain::Social-scoped Infra so domain_revs.social advances.
        if let Some(infra) = social_infra {
            infra.bump();
        } else if let Some(signal) = snapshot_signal {
            signal.bump();
        } else {
            rev.fetch_add(1, Ordering::Relaxed);
        }
        serde_json::json!({"ok": true, "status": "refreshed"})
    } else {
        serde_json::json!({"ok": true, "status": "pending"})
    }
}

#[cfg(test)]
#[path = "social_handler_tests.rs"]
mod tests;
