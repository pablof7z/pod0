//! Composed state tree for the podcast app.
//!
//! This module is the heart of the god-root consolidation (design doc at
//! `docs/design/podcast-app-state-refactor.md`).  It replaces the
//! field-for-field mirror between `PodcastHandle` and
//! `PodcastHostOpHandler` with a single `Arc<PodcastAppState>` shared by
//! both seams.
//!
//! ## Migration status
//!
//! Step 0: scaffolding — `Slot<T,D>`, `Durability`, `Infra`, and an *empty*
//! `PodcastAppState` (holds only `infra`).  Both god-structs still own their
//! old fields; the new `state` field is added alongside but unused.
//!
//! Step 1: Knowledge substate — `KnowledgeState` owns the two knowledge
//! `Arc`s, which are removed from both god-structs in the same PR.
//!
//! Step 3: Picks substate — `PicksState` owns `picks` + `score_in_progress`;
//! the duplicate guard on `FeedFetchCoordinator` is consolidated here.
//!
//! Step 4: Categories substate — `CategoriesState` owns `categories` cache +
//! `in_progress`; the duplicate guard on `FeedFetchCoordinator` is consolidated.
//!
//! Step 5a: Clips substate — `ClipsState` owns `clips` slot.
//! Step 5b: Transcripts substate — `TranscriptsState` owns `cache` slot.
//! Step 6:  Tasks substate — `TasksState` owns `tasks` slot + write-through
//!          persistence via `store::agent_tasks`.
//!
//! Step 7: Inbox substate — `dismissed_episode_ids`/`inbox_triage_cache`/
//!         `inbox_triage_in_progress` removed from both god-structs.
//! Steps 8-10 done (Comments, Discovery, Social).
//! Step 11: AgentChat substate — `conversation`/`agent_busy`/`agent_touched`
//!          removed from both god-structs.
//! Step 15: LibraryState substate — `store` + `identity` relocated from
//!          register.rs locals into `state.library`; removed from both
//!          god-structs.  All other substates keep their existing Arc clones.

pub mod agent_chat;
pub mod bump;
pub mod categories;
pub mod clips;
pub mod comments;
pub mod discovery;
pub mod domain;
pub mod friends;
pub mod inbox;
pub mod knowledge;
pub mod library;
pub mod notes;
pub mod picks;
pub mod playback;
pub mod publish;
pub mod slot;
pub mod social;
pub mod tasks;
pub mod transcripts;
pub mod voice;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::runtime::Runtime;

use crate::snapshot_signal::SnapshotUpdateSignal;

pub use bump::BumpHandle;
pub use domain::{Domain, DomainRevs};
pub use slot::{Derived, Durability, Persisted, Session, Slot};

// ── Infra ─────────────────────────────────────────────────────────────────────

/// Cross-cutting infrastructure every substate needs to bump the snapshot and
/// spawn off-actor work.
///
/// `Infra` is cheap to clone (three `Arc` clones + one shared `DomainRevs`)
/// and is injected into every substate at construction so substate methods
/// need no extra parameters to bump `rev`.
///
/// ## Rev-bump discipline
///
/// Call `infra.bump()` **after** releasing all `Slot` guards (never hold a
/// guard across the bump — `bump()` posts on the actor channel, which can
/// cause priority inversion with the actor thread).
#[derive(Clone)]
pub struct Infra {
    pub rev: Arc<AtomicU64>,
    pub(crate) signal: Option<SnapshotUpdateSignal>,
    pub runtime: Arc<Runtime>,
    /// Per-domain revision counters for push-side delta projections.
    pub domain_revs: Arc<DomainRevs>,
    /// The push domain this `Infra` handle is scoped to. Every `bump()` on this
    /// handle advances `domain_revs.counter(domain)` in addition to the global
    /// rev. Substates receive a `Domain`-scoped clone at construction (see
    /// [`Infra::with_domain`]); the un-scoped root `Infra` defaults to
    /// [`Domain::Misc`].
    pub domain: Domain,
}

impl Infra {
    /// Return a clone of this `Infra` scoped to a different push [`Domain`].
    ///
    /// Used by [`PodcastAppState::new_with_identity`] to hand each substate an
    /// `Infra` whose bare `bump()` routes to that substate's domain rev. All
    /// `Arc` fields are shared (cheap clone); only `domain` differs.
    pub fn with_domain(&self, domain: Domain) -> Self {
        Self {
            rev: self.rev.clone(),
            signal: self.signal.clone(),
            runtime: self.runtime.clone(),
            domain_revs: self.domain_revs.clone(),
            domain,
        }
    }

    /// Bump the snapshot rev — both the global rev AND this handle's domain rev.
    ///
    /// The global rev drives the pull path (`nmp_app_podcast_snapshot`) and the
    /// actor's `MarkChangedSinceEmit` coalescing. The domain rev drives the
    /// push-side per-domain typed sidecar (the closure emits the sidecar only
    /// when its domain rev advanced). A mutation on a `Domain::Playback`-scoped
    /// `Infra` therefore fires the `podcast.playback` sidecar on the next tick
    /// and leaves `podcast.library` / `podcast.settings` untouched.
    ///
    /// When a `SnapshotUpdateSignal` is wired (production), the global side
    /// posts `MarkChangedSinceEmit` on the actor channel, which coalesces
    /// multiple in-flight bumps into one rev increment. Without a signal
    /// (tests) it falls back to a raw `fetch_add(1, Relaxed)`.
    ///
    /// Replaces the `match self.snapshot_signal { Some(s)=>s.bump(), … }`
    /// pattern repeated in ~12 handlers.
    pub fn bump(&self) {
        // Delegate to a `BumpHandle` so the bump logic lives in exactly ONE
        // place.  `BumpHandle` holds NO `Arc<Runtime>`, so a task spawned ON the
        // runtime can capture a bump handle without pinning the runtime alive
        // (the UAF that a full `Infra` capture would create — see
        // [`Self::bump_handle`]).
        self.bump_handle().bump();
    }

    /// Build a [`BumpHandle`] — a clonable bump primitive that carries the
    /// rev/signal/domain-rev counters but **not** the `Arc<Runtime>`.
    ///
    /// A background task spawned on `infra.runtime` MUST NOT capture a full
    /// `Infra` (which holds `runtime: Arc<Runtime>`): that makes the task hold a
    /// strong ref to the runtime it runs on, so the runtime never drops, the
    /// task never stops, and at teardown it dereferences a freed `NmpApp`
    /// (use-after-free).  Capture a `BumpHandle` instead — it bumps the snapshot
    /// without keeping the runtime alive.
    pub fn bump_handle(&self) -> BumpHandle {
        BumpHandle::new(
            self.rev.clone(),
            self.signal.clone(),
            self.domain_revs.clone(),
            self.domain,
        )
    }

    /// Bump only when `changed` is true — avoids a no-op bump that would
    /// cause a snapshot rebuild for nothing.
    pub fn bump_if(&self, changed: bool) {
        if changed {
            self.bump();
        }
    }

    /// Bump a SPECIFIC domain's rev (plus the global rev), regardless of the
    /// domain this handle is scoped to.
    ///
    /// Most mutations should use the domain-scoped [`Self::bump`]. This explicit
    /// form is for the rare site that must target a domain other than its own
    /// `Infra` scope (e.g. a library mutation that also affects the widget
    /// badge), or for tests asserting a specific counter.
    pub fn bump_domain_explicit(&self, domain: Domain) {
        self.domain_revs
            .counter(domain)
            .fetch_add(1, Ordering::Relaxed);
        match &self.signal {
            Some(s) => s.bump(),
            None => {
                self.rev.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Read the current rev (for test assertions).
    #[cfg(test)]
    pub fn rev(&self) -> u64 {
        self.rev.load(Ordering::Relaxed)
    }

    /// Construct a minimal `Infra` suitable for unit tests (no signal, bare
    /// single-thread runtime so test code doesn't need a full multi-thread
    /// scheduler).
    #[cfg(test)]
    pub fn for_test() -> Self {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test tokio runtime");
        Self {
            rev: Arc::new(AtomicU64::new(1)),
            signal: None,
            runtime: Arc::new(rt),
            domain_revs: Arc::new(DomainRevs::new()),
            domain: Domain::Misc,
        }
    }

    /// Like `for_test()` but shares the caller-supplied `rev` Arc so tests can
    /// observe bumps via the same handle they hold.
    #[cfg(test)]
    pub fn for_test_with_rev(rev: Arc<AtomicU64>) -> Self {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test tokio runtime");
        Self {
            rev,
            signal: None,
            runtime: Arc::new(rt),
            domain_revs: Arc::new(DomainRevs::new()),
            domain: Domain::Misc,
        }
    }
}

// ── PodcastAppState ───────────────────────────────────────────────────────────

/// The single composed root owning every per-feature substate.
///
/// One `Arc<PodcastAppState>` is shared by the reader seam
/// (`PodcastHandle`) and the writer seam (`PodcastHostOpHandler`).
///
/// ## Migration status
///
/// Step 0: holds only `infra` and `knowledge`.  Remaining substates will be
/// added in Steps 2-N per the design doc.  At each step the corresponding
/// god-struct fields are REMOVED in the same PR (no overlap window).
///
/// Step 5a: clips substate added; `clips` field removed from both god-structs.
///
/// Step 5b: transcripts substate added; `transcripts` field removed from both
/// god-structs.
///
/// Step 6: tasks substate added; `agent_tasks` field removed from both
/// god-structs.
///
/// Step 7: inbox substate added; `dismissed_episode_ids`,
/// `inbox_triage_cache`, and `inbox_triage_in_progress` removed from both
/// god-structs.
///
/// Steps 8-10: comments, discovery, social substates added.
///
/// Step 11: agent_chat substate added; `conversation`/`agent_busy`/`agent_touched`
/// removed from both god-structs.
///
/// `picks` and `categories` are wrapped in `Arc` so `FeedFetchCoordinator` can
/// hold the SAME substate instance (canonical single guard — no duplicate Arcs).
pub struct PodcastAppState {
    /// Cross-cutting infrastructure (rev + signal + runtime).
    pub infra: Infra,

    /// Library substate (Step 15).  Owns the canonical persisted root:
    /// `store` (`Arc<Mutex<PodcastStore>>`) and `identity`
    /// (`Arc<Mutex<IdentityStore>>`).  All other substates hold existing Arc
    /// clones; `LibraryState` is the tree-level owner.
    pub library: library::LibraryState,

    /// Knowledge substate (Step 1).
    pub knowledge: knowledge::KnowledgeState,

    /// Picks substate (Step 3).  Owns picks slot + the single scoring guard.
    /// Wrapped in `Arc` so `FeedFetchCoordinator` can hold the canonical instance.
    pub picks: Arc<picks::PicksState>,

    /// Categories substate (Step 4).  Owns categories cache + single guard.
    /// Wrapped in `Arc` so `FeedFetchCoordinator` can hold the canonical instance.
    pub categories: Arc<categories::CategoriesState>,

    /// Clips substate (Step 5a).  Owns the in-memory clip list.
    pub clips: clips::ClipsState,

    /// Transcripts substate (Step 5b).  Owns the per-episode transcript cache.
    pub transcripts: transcripts::TranscriptsState,

    /// Tasks substate (Step 6).  Owns agent-task list + write-through persistence.
    pub tasks: tasks::TasksState,

    /// Local notes substate. Owns user/agent notes and write-through persistence.
    pub notes: notes::NotesState,

    /// Local friends substate. Owns user-curated friends.
    pub friends: friends::FriendsState,

    /// Inbox substate (Step 7).  Owns `dismissed` + `triage_cache` slots +
    /// `triage_in_progress` atomic.  Tokio tasks write back scores via
    /// `triage_cache.share()`.
    ///
    /// Wrapped in `Arc` so `FeedFetchCoordinator` can hold the canonical
    /// instance and call `maybe_enqueue_triage` from the transport thread
    /// after an async subscribe delivers fresh episodes (D8 re-homing).
    pub inbox: Arc<inbox::InboxState>,

    /// Comments substate (Step 8).  Owns cache + viewed-episode-id slots.
    /// `CommentsObserver` shares `cache` off the actor thread via `.share()`.
    pub comments: comments::CommentsState,

    /// Discovery substate (Step 9).  Owns iTunes + Nostr results slots.
    /// `NostrDiscoveryObserver` shares `nostr_results` off the actor thread via
    /// `.share()`.  Removes the dead-duplicate `nostr_results` handler Arc.
    pub discovery: discovery::DiscoveryState,

    /// Social substate (Step 10).  Owns `social_slot` + `agent_notes` slots.
    /// `AgentNotesObserver` shares `agent_notes` off the actor thread via
    /// `.share()`.  Removes the dead-duplicate `agent_notes` handler Arc.
    pub social: social::SocialState,

    /// AgentChat substate (Step 11).  Owns the conversation transcript +
    /// `agent_busy` + `agent_touched` flags.  Wraps `AgentChatHandler` so
    /// the LLM dispatch logic stays in one place.
    pub agent_chat: agent_chat::AgentChatState,

    /// Voice substate (Step 12).  Owns `voice_state` projection + the
    /// `VoiceConversationManager` (LLM↔TTS loop).
    ///
    /// **Shutdown fence**: `nmp_app_podcast_unregister` MUST call
    /// `state.voice.shutdown()` before dropping the handle.  This fences
    /// in-flight Tokio tasks that hold a `*mut NmpApp` deref from
    /// completing after `nmp_app_free`.  The ordering is identical to the
    /// previous `reclaimed.voice_conversation.shutdown()` call.
    pub voice: voice::VoiceSubstate,

    /// Publish substate (Step 13).  Owns the NIP-F4 per-podcast keypairs
    /// (`podcast_keys`, Persisted) and the diagnostic publish map
    /// (`publish_state`, Session).
    pub publish: publish::PublishState,

    /// Playback substate (Step 14).  Owns `player_actor` (Session),
    /// `queue` (Persisted, write-through to store.cached_queue), and
    /// `download_queue` (Session).
    ///
    /// Cross-thread: the report FFIs (`audio_report`, `download_report`)
    /// write here from the platform audio/download threads via `.share()`.
    pub playback: playback::PlaybackState,

    /// In-app feedback runtime (Step 16).  Moved from the god-struct mirrors
    /// (`PodcastHandle.feedback` / `PodcastHostOpHandler.feedback`) into the
    /// shared state tree so both seams read the same event cache without a
    /// separate Arc wire.
    ///
    /// `nmp-feedback` owns the relay-pinned subscription, publish tags, event
    /// cache, and thread projection.  Empty until the first `FetchFeedback`
    /// dispatch.
    pub feedback: nmp_feedback::FeedbackRuntime,

    /// Optimistic-subscribe async feed-fetch coordinator (Step 16).  Moved
    /// from both god-structs into the shared tree.  The handler registers a
    /// pending fetch + dispatches the async HTTP command; the handle's
    /// HTTP-report FFI resolves the result.  Holds a shared clone of
    /// `library.store` + `infra.rev` + signal.
    pub(crate) feed_fetch: std::sync::Arc<crate::feed_fetch::FeedFetchCoordinator>,
}

impl PodcastAppState {
    /// Construct the composed state from shared infra and a store clone.
    ///
    /// Each substate seeds its own slots internally and clones `infra`
    /// plus the shared `store` Arc.  The 31-arg positional constructor in
    /// `register.rs` will be replaced by this single call once all steps
    /// are complete.
    ///
    /// Uses a stub `FeedbackRuntime` with the podcast project coordinate.
    pub fn new(
        infra: Infra,
        store: Arc<std::sync::Mutex<crate::store::PodcastStore>>,
    ) -> Self {
        let feedback = nmp_feedback::FeedbackRuntime::new(
            nmp_feedback::FeedbackConfig::new(crate::PODCAST_FEEDBACK_PROJECT_COORDINATE)
                .with_interest_namespace(crate::PODCAST_FEEDBACK_INTEREST_NAMESPACE),
            std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            infra.rev.clone(),
        );
        Self::new_with_identity(
            infra,
            store,
            Arc::new(std::sync::Mutex::new(
                crate::store::identity::IdentityStore::new(),
            )),
            feedback,
        )
    }

    /// Full constructor accepting an externally-created identity store so
    /// `register.rs` can pass the shared Arc rather than creating a new one.
    ///
    /// Step 16: `feedback` is also injected — `FeedbackRuntime` needs a
    /// project coordinate + relay seed from the app layer and a
    /// `with_snapshot_bump` hook that references the live signal.
    /// `FeedFetchCoordinator` is constructed internally since it only needs
    /// `picks` + `categories` + `infra` which are all available here.
    pub fn new_with_identity(
        infra: Infra,
        store: Arc<std::sync::Mutex<crate::store::PodcastStore>>,
        identity: Arc<std::sync::Mutex<crate::store::identity::IdentityStore>>,
        feedback: nmp_feedback::FeedbackRuntime,
    ) -> Self {
        // Step 15: LibraryState owns the store + identity Arcs.  All other
        // substates receive clones of these same Arcs (lock topology unchanged).
        //
        // Domain scoping (perf/domain-rev-wiring-substates): each substate gets
        // an `Infra` scoped to its push domain so its bare `infra.bump()` routes
        // to the right domain rev. The `Domain → substate` mapping lives HERE
        // (one place); the `Domain → counter` mapping lives in
        // `DomainRevs::counter`. Substates not yet split into their own push
        // domain keep `Domain::Misc` (the default `infra.domain`).
        //   - PlaybackState        → Playback (now_playing + queue + downloads work)
        //   - CategoriesState      → Library  (categories are part of the library payload)
        //   - everything else      → Misc
        let library = library::LibraryState::new(store.clone(), identity.clone());
        let knowledge = knowledge::KnowledgeState::new(infra.clone(), store.clone());
        let picks = Arc::new(picks::PicksState::new(infra.clone(), store.clone()));
        let categories = Arc::new(categories::CategoriesState::new(
            infra.with_domain(Domain::Library),
            store.clone(),
        ));
        let clips = clips::ClipsState::new(infra.clone(), store.clone());
        let transcripts = transcripts::TranscriptsState::new(infra.clone(), store.clone());
        let tasks = tasks::TasksState::new(infra.with_domain(Domain::Tasks), store.clone());
        let notes = notes::NotesState::new(infra.clone(), store.clone());
        let friends = friends::FriendsState::new(infra.with_domain(Domain::Social), store.clone());
        let inbox = Arc::new(inbox::InboxState::new(infra.clone(), store.clone()));
        let comments =
            comments::CommentsState::new(infra.clone(), store.clone(), identity.clone());
        let discovery = discovery::DiscoveryState::new(infra.clone());
        let social = social::SocialState::new(infra.with_domain(Domain::Social));
        let agent_chat = agent_chat::AgentChatState::new(infra.clone(), store.clone());
        // Voice is constructed with a null app pointer by default.  In
        // production (`register.rs`) the caller replaces this field before
        // wrapping in `Arc` using `with_voice`.
        let voice = voice::VoiceSubstate::new(
            infra.with_domain(Domain::Voice),
            store.clone(),
            std::ptr::null_mut(),
        );
        let publish = publish::PublishState::new(infra.clone(), store.clone());
        let playback = playback::PlaybackState::new(
            infra.with_domain(Domain::Playback),
            store.clone(),
        );
        // Step 16: FeedFetchCoordinator is constructed inside the state — it
        // needs picks + categories + inbox Arcs available here, plus infra.rev + signal.
        // inbox Arc added (D8 re-homing): apply_subscribe_result enqueues triage after
        // fresh episodes land, matching the synchronous refresh path.
        let feed_fetch = std::sync::Arc::new(crate::feed_fetch::FeedFetchCoordinator::new(
            store.clone(),
            infra.rev.clone(),
            infra.signal.clone(),
            Arc::clone(&categories),
            Arc::clone(&picks),
            Arc::clone(&inbox),
        ));
        Self {
            infra,
            library,
            knowledge,
            picks,
            categories,
            clips,
            transcripts,
            tasks,
            notes,
            friends,
            inbox,
            comments,
            discovery,
            social,
            agent_chat,
            voice,
            publish,
            playback,
            feedback,
            feed_fetch,
        }
    }
}
