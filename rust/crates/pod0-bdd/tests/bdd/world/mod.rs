//! `PodWorld` — one fresh world per scenario: a fresh temp directory, a
//! freshly prepared authoritative store, and a real `Pod0Facade` opened over
//! it — the exact seven-operation surface Swift and Kotlin see
//! (`rust/FACADE_CONTRACT.md`), never the application layer underneath.
//!
//! Everything is staged lazily: a `Given` records plain data (what feed
//! exists at what URL) and touches no file, and the first step that needs the
//! facade calls [`PodWorld::ensure_started`], which prepares the store and
//! opens the facade. A scenario that never acts therefore never pays for —
//! or accidentally depends on — a running core.
//!
//! THIS FILE OWNS THE STATE AND NOTHING ELSE. `PodWorld` is one struct with
//! one lifetime (the scenario), so its fields are declared in one place to be
//! read as a whole; its BEHAVIOUR splits by the phase of a scenario it
//! serves, and each phase is a private sibling module below. Rust module
//! privacy makes the split free: the fields stay private to `world`, every
//! child module still reaches them, and nothing leaks to `steps`.
//!
//! - `store` — the fresh-install store-preparation ritual, mirroring the
//!   production Swift bootstrap through public facade exports only.
//! - `staging` — `Given`-time staging (plain data, no I/O) and the single
//!   lazy `ensure_started` that turns it into a running facade.
//! - `actions` — `When`-time acts: dispatch a command, play the host role
//!   for one bounded request, cancel, relaunch.
//! - `observe` — the observation plane every `Then` reads: projection
//!   snapshots, the recording live subscriber, drained host work, receipts.

mod actions;
mod observe;
mod staging;
mod store;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use pod0_application::{CommandEnvelope, HostObservationReceipt, LeasedHostRequestEnvelope};
use pod0_domain::{StateRevision, SubscriptionId};
use pod0_facade::Pod0Facade;

use self::observe::RecordingSubscriber;
use self::staging::StagedFeed;

/// A subscribe command the scenario issued, kept verbatim so a retry step
/// can re-dispatch the byte-identical envelope and a `Then` can find the
/// operation it produced.
struct IssuedSubscribe {
    envelope: CommandEnvelope,
}

/// The prepared store and the temp directory that owns it. The directory
/// handle is held so the store outlives a relaunch but dies with the world.
struct StoreFixture {
    _directory: tempfile::TempDir,
    path: PathBuf,
}

/// One open live library view: the transient handle `subscribe` returned and
/// the recording subscriber deliveries land on.
struct LibraryWatch {
    subscription_id: SubscriptionId,
    subscriber: Arc<RecordingSubscriber>,
}

#[derive(cucumber::World, Default)]
pub struct PodWorld {
    /// Fixture feeds keyed by URL — what "the host" finds when the core
    /// asks it to fetch that URL, rendered at fetch time.
    staged_feeds: BTreeMap<String, StagedFeed>,

    store: Option<StoreFixture>,
    facade: Option<Arc<Pod0Facade>>,

    /// Subscribe commands by feed URL, so prose can keep naming the URL.
    subscribes: BTreeMap<String, IssuedSubscribe>,
    /// Feed-fetch work the host has accepted (drained) but not yet answered.
    accepted_fetches: BTreeMap<String, LeasedHostRequestEnvelope>,
    /// The new-episode announcement the host accepted but has not delivered.
    accepted_announcement: Option<LeasedHostRequestEnvelope>,
    /// The receipt the facade returned for the most recent host observation.
    last_receipt: Option<HostObservationReceipt>,

    library_watch: Option<LibraryWatch>,
    /// Deliveries the live view had received when the app closed it.
    deliveries_at_close: Option<usize>,

    /// The state revision noted when the app cancelled — what "has not
    /// advanced since" measures from.
    revision_at_cancel: Option<StateRevision>,

    /// Monotonic fixture counter: command identities and observation times
    /// both derive from it, so every identity in a scenario is distinct and
    /// every timestamp is deterministic. No injected clock.
    counter: u64,
}

impl std::fmt::Debug for PodWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PodWorld")
            .field(
                "staged_feeds",
                &self.staged_feeds.keys().collect::<Vec<_>>(),
            )
            .field("started", &self.facade.is_some())
            .field("subscribes", &self.subscribes.keys().collect::<Vec<_>>())
            .field("watch_open", &self.library_watch.is_some())
            .finish()
    }
}

impl PodWorld {
    /// The running facade — every `When` action and every observation goes
    /// through here, so "the facade must be running first" is stated once.
    fn facade(&self) -> &Arc<Pod0Facade> {
        self.facade
            .as_ref()
            .expect("pod0-bdd: the facade must be running (ensure_started) before use")
    }
}
