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
//! time** in [`crate::state::social::SocialState::agent_notes_snapshot`], by
//! applying the shared [`nmp_nip02::ActiveFollowSet`] predicate to each cached
//! note's author hex.  This observer module only materialises the follow-list
//! snapshot; it does NOT stamp `trusted`.  See `agent_note_handler.rs` for why
//! the verdict must be recomputed at projection (follow/unfollow must flip the
//! verdict on every existing note, with no stale freeze).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use nostr::nips::nip19::ToBech32;
use nmp_core::substrate::KernelEvent;
use nmp_core::KernelEventObserver;
use nmp_nip02::FollowListProjection;

use crate::ffi::projections::{ContactSummary, SocialSnapshot};
use crate::snapshot_signal::SnapshotUpdateSignal;

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
}

impl FollowListObserver {
    /// Construct the observer.
    ///
    /// * `active_pubkey` — the kernel's shared active-account slot
    ///   (`NmpApp::active_account_handle()`).
    /// * `social_slot` — the shared slot written by this observer and read by
    ///   the snapshot projection.
    /// * `rev` — shared rev counter; bumped on every kind:3 event when no
    ///   `snapshot_signal` is present.
    pub fn new(
        active_pubkey: Arc<Mutex<Option<String>>>,
        social_slot: Arc<Mutex<Option<SocialSnapshot>>>,
        rev: Arc<AtomicU64>,
    ) -> Self {
        Self {
            projection: FollowListProjection::new(active_pubkey),
            social_slot,
            rev,
            snapshot_signal: None,
        }
    }

    /// Attach a `SnapshotUpdateSignal` so the observer can push frames to the
    /// iOS shell immediately without waiting for the next poll tick.
    pub fn with_snapshot_signal(mut self, signal: SnapshotUpdateSignal) -> Self {
        self.snapshot_signal = Some(signal);
        self
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

        // Delegate to the upstream FollowListProjection.  It applies the author
        // gate (only kind:3 from the active account updates its map), so we
        // ask for the snapshot only after it has had a chance to update.
        self.projection.on_kernel_event(event);

        let snap = self.projection.snapshot();

        // Materialise ContactSummary rows with bech32 npubs.
        // The inner FollowListProjection stores raw hex pubkeys (aim.md §2 —
        // presentation in the app layer).  We bech32-encode here since
        // ContactSummary is the typed shell DTO.
        let contacts: Vec<ContactSummary> = snap
            .follows
            .iter()
            .map(|entry| {
                let npub = nostr::PublicKey::parse(&entry.pubkey)
                    .ok()
                    .and_then(|pk| pk.to_bech32().ok())
                    .unwrap_or_else(|| entry.pubkey.clone());
                ContactSummary {
                    npub,
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
            });
        }

        // Signal the shell.
        if let Some(signal) = &self.snapshot_signal {
            signal.bump();
        } else {
            self.rev.fetch_add(1, Ordering::Relaxed);
        }
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
/// rev and signals the shell; if not yet populated, returns
/// `{"ok":true,"status":"pending"}` — the observer will deliver when kind:3
/// arrives.
pub fn handle_fetch_contacts(
    social: Arc<Mutex<Option<SocialSnapshot>>>,
    rev: Arc<AtomicU64>,
    snapshot_signal: Option<&SnapshotUpdateSignal>,
) -> serde_json::Value {
    let has_data = social.lock().ok().and_then(|s| s.clone()).is_some();
    if has_data {
        // Already populated — bump so the shell re-renders the existing data.
        if let Some(signal) = snapshot_signal {
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
