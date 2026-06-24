//! Per-domain snapshot revision counters and the [`Domain`] tag.
//!
//! Split out of `state/mod.rs` to keep that file under the 500-line hard ceiling
//! (AGENTS.md). [`DomainRevs`] holds the seven `Arc<AtomicU64>` counters that
//! drive the per-domain push-projection deltas; [`Domain`] is the tag a
//! substate's [`Infra`](super::Infra) is scoped to so its bare `bump()` routes
//! to the right counter.

use std::sync::atomic::AtomicU64;
use std::sync::Arc;

// ── DomainRevs ────────────────────────────────────────────────────────────────

/// Per-domain snapshot revision counters.
///
/// Each counter tracks when a specific domain's state last changed.
/// The push-side typed-projection closures compare against `last_emitted`
/// and return `None` (omit the sidecar) when unchanged — giving true
/// per-domain delta semantics without re-serialising the whole snapshot.
///
/// Every domain-specific mutation MUST bump its domain rev **and** the
/// global rev (`infra.rev`) so the pull path continues to work correctly.
/// A `Domain`-scoped [`Infra::bump`](super::Infra::bump) does both atomically.
///
/// ## Domain assignment
///
/// | Counter | Domain / Key | State sources |
/// |---|---|---|
/// | `library` | `podcast.library` | store (podcasts + episodes + categories + inbox) |
/// | `playback` | `podcast.playback` | now_playing + queue |
/// | `downloads` | `podcast.downloads` | download_queue |
/// | `settings` | `podcast.settings` | store settings + relays |
/// | `identity` | `podcast.identity` | identity store |
/// | `widget` | `podcast.widget` | player state + library (derived) |
/// | `social` | `podcast.social` | social graph + agent notes + nostr conversations |
/// | `voice` | `podcast.voice` | voice mode state; bumped on voice report |
/// | `misc` | `podcast.misc` | everything else (wiki/picks/clips/transcripts/…) |
/// | `tasks` | (internal) | agent-task list mutations; used for test assertions |
#[derive(Clone)]
pub struct DomainRevs {
    pub library: Arc<AtomicU64>,
    pub playback: Arc<AtomicU64>,
    pub downloads: Arc<AtomicU64>,
    pub settings: Arc<AtomicU64>,
    pub identity: Arc<AtomicU64>,
    pub widget: Arc<AtomicU64>,
    pub social: Arc<AtomicU64>,
    pub voice: Arc<AtomicU64>,
    pub misc: Arc<AtomicU64>,
    /// Tasks domain rev — advanced when the kernel-owned periodic tick fires
    /// due tasks.  There is no `podcast.tasks` push-sidecar yet (tasks ride
    /// `podcast.misc`); this counter lets test assertions distinguish a
    /// tasks-domain bump from an unrelated `misc` mutation.
    pub tasks: Arc<AtomicU64>,
}

impl Default for DomainRevs {
    fn default() -> Self {
        Self::new()
    }
}

impl DomainRevs {
    pub fn new() -> Self {
        // Start at 1 so the first emit always fires (closures start last_emitted at 0).
        Self {
            library: Arc::new(AtomicU64::new(1)),
            playback: Arc::new(AtomicU64::new(1)),
            downloads: Arc::new(AtomicU64::new(1)),
            settings: Arc::new(AtomicU64::new(1)),
            identity: Arc::new(AtomicU64::new(1)),
            widget: Arc::new(AtomicU64::new(1)),
            social: Arc::new(AtomicU64::new(1)),
            voice: Arc::new(AtomicU64::new(1)),
            misc: Arc::new(AtomicU64::new(1)),
            tasks: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Resolve the `Arc<AtomicU64>` counter for a given [`Domain`].
    ///
    /// This is the ONE place the `Domain → counter` mapping lives. Every
    /// substate is constructed with a [`Domain`]-scoped [`Infra`](super::Infra);
    /// its bare `infra.bump()` calls route here automatically.
    pub fn counter(&self, domain: Domain) -> &Arc<AtomicU64> {
        match domain {
            Domain::Library => &self.library,
            Domain::Playback => &self.playback,
            Domain::Downloads => &self.downloads,
            Domain::Settings => &self.settings,
            Domain::Identity => &self.identity,
            Domain::Widget => &self.widget,
            Domain::Social => &self.social,
            Domain::Voice => &self.voice,
            Domain::Misc => &self.misc,
            Domain::Tasks => &self.tasks,
        }
    }
}

/// The push-projection domain a substate's mutations belong to.
///
/// Each substate is constructed with a [`Domain`]-scoped [`Infra`](super::Infra)
/// (via [`Infra::with_domain`](super::Infra::with_domain)). When the substate
/// calls `self.infra.bump()`, the bump routes to BOTH its domain rev counter AND
/// the global rev — so an actual mutation advances only its own domain's delta.
/// The mapping is centralised in
/// [`PodcastAppState::new_with_identity`](super::PodcastAppState::new_with_identity)
/// (which `Domain` each substate gets) and [`DomainRevs::counter`] (which counter
/// each `Domain` resolves to).
///
/// `Misc` is the default catch-all for substates not yet split into their own
/// push domain (wiki/picks/clips/transcripts/agent/discovery/publish/comments/knowledge).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Domain {
    Library,
    Playback,
    Downloads,
    Settings,
    Identity,
    Widget,
    /// Social graph, agent notes, and Nostr conversation threads.
    /// Bumped whenever `SocialState` is mutated (inbound note cached,
    /// outbound turn recorded, or follow list updated).
    Social,
    /// Voice mode state.  Bumped whenever a voice report arrives.
    Voice,
    Misc,
    /// Agent-task list domain.  The kernel-owned periodic tick fires
    /// `maybe_run_due_tasks` every 60 s and bumps this counter when at least
    /// one task was dispatched.  Tasks still ride the `podcast.misc` push
    /// sidecar; this counter exists for precise test assertions (a test
    /// asserting only the global rev cannot distinguish a tasks-tick from an
    /// unrelated misc mutation).
    Tasks,
}
