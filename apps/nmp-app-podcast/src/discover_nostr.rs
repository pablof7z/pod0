//! `podcast.discover_nostr` — NIP-F4 (`kind:10154`) podcast discovery via
//! NMP's relay pool, the canonical `EnsureInterest` + `KernelEventObserver`
//! pattern.
//!
//! ## Why this replaced the old capability-dispatch path
//!
//! The previous implementation dispatched a `nostr_relay` capability request
//! to the iOS shell, which opened its own `URLSessionWebSocketTask`. That is
//! wrong: **NMP core owns all relay connections.** The iOS shell must never
//! open a relay socket. The correct pattern (NMP v0.2.0; the
//! `nmp-nip01::visible_relations` action-triggered-subscription template —
//! the shape NMP PR #898 / builder-guide ch. 28 describe) is:
//!
//! 1. The action emits [`nmp_core::ActorCommand::EnsureInterest`]. The kernel
//!    opens the subscription through its own relay pool using the app's
//!    configured relays + the user's NIP-65 outbox read relays. No relay URL
//!    is specified — NMP routes automatically. `is_indexer_discovery = true`
//!    routes the sweep through the indexer because `kind:10154` is sparse.
//! 2. A [`NostrDiscoveryObserver`] registered at init fires for every inbound
//!    `kind:10154` event ([`KernelEventObserver::on_kernel_event`]).
//! 3. The observer parses the event into a [`NostrShowSummary`] and writes it
//!    into the shared `nostr_results` slot, bumping `rev`.
//! 4. The existing snapshot projection reads `nostr_results` unchanged.
//!
//! ## Lifecycle
//!
//! The action is ref-counted by `(owner, key, scope)` via [`SubIdentity`]: a
//! `Claim` attaches one owner (the view), a `Release` detaches it. The kernel
//! keeps one live subscription while any owner is attached. The view claims on
//! appear and releases on disappear (see `NostrDiscoverForm.swift`).
//!
//! ## Doctrine
//!
//! * **D0** — `nmp-core` never names podcast nouns; the kernel emits raw
//!   `KernelEvent`s and this per-app module composes the typed view.
//! * **D6** — the observer fires best-effort: a poisoned slot or an event that
//!   fails to parse is a silent no-op.
//! * **D7** — the kernel's relay pool performs the I/O; this module only
//!   declares the interest and parses results. iOS executes nothing here.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use nmp_planner::interest::{InterestId, InterestLifecycle, InterestScope, LogicalInterest};
use nmp_planner::stable_hash::stable_hash64;
use nmp_core::subs::{SubIdentity, SubKey, SubOwnerKey, SubScope};
use nmp_core::substrate::{KernelEvent, ViewDependencies};
use nmp_core::KernelEventObserver;

use podcast_discovery::{parse_kind_10154, NipF4Show};

use crate::ffi::projections::NostrShowSummary;
use crate::snapshot_signal::SnapshotUpdateSignal;

/// NIP-F4 podcast show event kind.
pub const KIND_NIP_F4_SHOW: u32 = 10154;

/// Maximum number of historical shows to fetch per discovery sweep.
pub const NOSTR_DISCOVERY_LIMIT: u32 = 50;

/// Namespace discriminant folded into every stable hash so the discovery
/// interest / owner / key never collide with another subsystem's ids.
const NOSTR_DISCOVERY_NAMESPACE: &str = "podcast.discover_nostr";

/// Stable, deterministic [`InterestId`] for the discovery sweep.
///
/// The same logical interest always hashes to the same id, so re-registering
/// (e.g. opening the form twice) de-dupes to one live subscription in the
/// kernel registry rather than spawning a second REQ.
#[must_use]
pub fn nostr_discovery_interest_id() -> InterestId {
    InterestId(stable_hash64(NOSTR_DISCOVERY_NAMESPACE))
}

/// Build the [`LogicalInterest`] for a NIP-F4 discovery sweep.
///
/// Declares `kind:10154` with a bounded history limit. `InterestScope::Global`
/// (the sweep is not tied to one account's mailbox) and
/// `InterestLifecycle::OneShot` (fetch-and-close — discovery is a browse, not
/// a live tail).
///
/// `is_indexer_discovery = true` is set on the returned interest:
/// `into_logical_interest` always defaults it to `false`, but `kind:10154` is
/// sparse across the relay network, so the sweep must route through the
/// indexer rather than the user's outbox relays.
#[must_use]
pub fn nostr_discovery_interest() -> LogicalInterest {
    let mut interest = ViewDependencies {
        kinds: vec![KIND_NIP_F4_SHOW],
        limit: Some(NOSTR_DISCOVERY_LIMIT),
        ..Default::default()
    }
    .into_logical_interest(
        nostr_discovery_interest_id(),
        InterestScope::Global,
        InterestLifecycle::OneShot,
    );
    // Sparse kind — route the sweep through the indexer, not outbox relays.
    interest.is_indexer_discovery = true;
    interest
}

/// Build the ref-counting [`SubIdentity`] for one discovery consumer.
///
/// `owner` is per-consumer (so two views can each Claim/Release independently);
/// `key` is shared across all consumers of the discovery sweep (so they
/// de-dupe onto one live subscription); `scope` is `Global`.
#[must_use]
pub fn nostr_discovery_identity(consumer_id: &str) -> SubIdentity {
    SubIdentity::new(
        SubOwnerKey::new(("podcast.discover_nostr.owner", consumer_id)),
        SubKey::new(NOSTR_DISCOVERY_NAMESPACE),
        SubScope::Global,
    )
}

/// Project a [`NipF4Show`] onto the FFI-wire [`NostrShowSummary`].
#[must_use]
pub fn project_show(show: &NipF4Show) -> NostrShowSummary {
    NostrShowSummary {
        event_id: show.event_id.clone(),
        author_pubkey: show.author_pubkey.clone(),
        title: show.title.clone(),
        description: show.description.clone(),
        feed_url: show.feed_url.clone(),
        artwork_url: show.artwork_url.clone(),
        categories: show.categories.clone(),
    }
}

/// Insert (or replace) one projected show in the shared slot, deduping by
/// `author_pubkey`, and bump `rev`. Returns `true` if the slot changed.
///
/// `kind:10154` is a replaceable event — one show per pubkey. The observer
/// fires again on a `Replaced` ingest (and a second consumer re-Claims the
/// same interest), so blind-appending would pile up duplicate rows. Keying on
/// `author_pubkey` makes a re-arrival update the existing row in place.
fn upsert_show(
    show: NostrShowSummary,
    slot: &Arc<Mutex<Vec<NostrShowSummary>>>,
    rev: &Arc<AtomicU64>,
    snapshot_signal: Option<&SnapshotUpdateSignal>,
) -> bool {
    let Ok(mut guard) = slot.lock() else {
        // D6 — poisoned slot is a silent no-op.
        return false;
    };
    match guard
        .iter_mut()
        .find(|existing| existing.author_pubkey == show.author_pubkey)
    {
        Some(existing) => {
            if *existing == show {
                return false; // no-op: identical re-arrival, don't churn rev.
            }
            *existing = show;
        }
        None => guard.push(show),
    }
    drop(guard);
    if let Some(signal) = snapshot_signal {
        signal.bump();
    } else {
        rev.fetch_add(1, Ordering::Relaxed);
    }
    true
}

/// In-process [`KernelEventObserver`] that turns inbound `kind:10154` events
/// into [`NostrShowSummary`] rows on the shared discovery slot.
///
/// Registered once at init (see `ffi::register`) against the same
/// `nostr_results` / `rev` slots the snapshot projection reads. Fires on the
/// kernel actor thread between relay frames, so [`Self::on_kernel_event`] does
/// only cheap, allocation-bounded work (a kind check, a parse, a slot upsert).
pub struct NostrDiscoveryObserver {
    /// Shared with `PodcastHandle.nostr_results` (snapshot reader) and
    /// `PodcastHostOpHandler.nostr_results` (legacy writer slot).
    nostr_results: Arc<Mutex<Vec<NostrShowSummary>>>,
    /// Shared monotonic snapshot revision; bumped when the slot changes so the
    /// next push frame reflects the new show.
    rev: Arc<AtomicU64>,
    snapshot_signal: Option<SnapshotUpdateSignal>,
}

impl NostrDiscoveryObserver {
    #[must_use]
    pub fn new(nostr_results: Arc<Mutex<Vec<NostrShowSummary>>>, rev: Arc<AtomicU64>) -> Self {
        Self {
            nostr_results,
            rev,
            snapshot_signal: None,
        }
    }

    pub(crate) fn with_snapshot_signal(mut self, snapshot_signal: SnapshotUpdateSignal) -> Self {
        self.snapshot_signal = Some(snapshot_signal);
        self
    }
}

impl KernelEventObserver for NostrDiscoveryObserver {
    fn on_kernel_event(&self, event: &KernelEvent) {
        if event.kind != KIND_NIP_F4_SHOW {
            return;
        }
        // `KernelEvent` carries `author` (not the nostr-standard `pubkey`) and
        // no `sig`, so serializing it to JSON for `parse_nip_f4_event_json`
        // would silently fail to parse. Use the field-level parser instead:
        // it maps 1:1 onto the substrate event fields.
        let Ok(show) = parse_kind_10154(
            event.kind,
            &event.id,
            &event.author,
            &event.content,
            &event.tags,
        ) else {
            return; // D6 — unparseable event is dropped silently.
        };
        let _ = upsert_show(
            project_show(&show),
            &self.nostr_results,
            &self.rev,
            self.snapshot_signal.as_ref(),
        );
    }
}

#[cfg(test)]
#[path = "discover_nostr_tests.rs"]
mod tests;
