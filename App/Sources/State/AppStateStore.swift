import Foundation
import Observation
import WidgetKit
import os.log

// MARK: - Friend invite

/// Pre-filled data for an incoming friend invite deep-link.
/// Consumed by `AgentFriendsView` to open `AddFriendSheet` with values already typed in.
struct PendingFriendInvite: Equatable, Identifiable {
    /// Bech32-encoded public key of the person being added (full `npub1…` string).
    let npub: String
    /// Display name suggested by the invite link; the user can change it before adding.
    let name: String?

    /// Stable identity derived from the public key — two invites for the same key are the same.
    var id: String { npub }
}

/// Single source of truth. All mutations route through here so the `didSet`
/// observer can persist automatically. UI and agent both call the same methods.
@MainActor
@Observable
final class AppStateStore {

    nonisolated private static let logger = Logger.app("AppStateStore")

    /// Set by UITestSeeder before AppStateStore is initialised so that
    /// `flushPendingPositions` writes the episode SQLite store synchronously
    /// (bypassing the background Task). This guarantees positions survive a
    /// SIGKILL force-quit during automated UI tests without making ALL writes
    /// synchronous (which would throttle the main thread at the 4 Hz kernel
    /// tick rate and break other P0-04 tests).
    nonisolated(unsafe) static var synchronousPositionFlushForUITests = false

    // MARK: - User identity

    /// The human user's Nostr identity (signer + Keychain key material + NIP-46
    /// connection). Owned here so the wiring layer (`AppStateStore+Notes`,
    /// `AppStateStore+Clips`) and the SwiftUI surface share one instance without
    /// a process-wide singleton. Call `identity.start()` once at launch
    /// (see `AppMain`) to load any persisted key / resume a remote signer.
    let identity = UserIdentityStore()

    // MARK: - Navigation

    /// Pending friend invite dispatched by a `podcastr://friend/add` deep-link.
    /// Consumed and cleared by `AgentFriendsView` on `.onChange` so it fires exactly once.
    var pendingFriendInvite: PendingFriendInvite?

    /// Transcript line the user long-pressed in the player. Drained by
    /// `AgentChatSession.init` and prefilled into the composer; cleared by
    /// the same call so a later sheet re-open starts blank.
    ///
    /// Kept around for the internal-only surfaces (clip composer, quote
    /// share) that still operate on transcript segments. The primary chapter
    /// long-press path now writes `pendingChapterAgentContext` instead — the
    /// user never sees transcript text.
    var pendingTranscriptAgentContext: TranscriptAgentContext?

    /// Chapter the user long-pressed in `PlayerChaptersScrollView`. Drained
    /// by `AgentChatSession.init` and prefilled into the composer; cleared
    /// by the same call so a later sheet re-open starts blank. Carries no
    /// transcript text — only the chapter title + time range; the agent
    /// fetches transcript context through its tool inventory.
    var pendingChapterAgentContext: ChapterAgentContext?

    /// Voice note the user recorded via the mic button in the player. Drained
    /// by `AgentChatSession.init` and auto-sent to the agent. The context
    /// carries the timestamp anchor, the active chapter bounds, and the
    /// transcribed utterance; the agent decides what to do with it.
    var pendingVoiceNoteAgentContext: VoiceNoteAgentContext?

    /// Counterparty pubkey of the most-recent Nostr conversation turn —
    /// drives the floating "Talking to X" capsule on the main screen.
    /// Cleared `nostrActivityIndicatorDuration` seconds after the last
    /// turn lands (each new turn resets the timer). Non-persisted UI state.
    var activeNostrCounterparty: String?

    /// Cancellable timer that clears `activeNostrCounterparty`. Ignored by
    /// the @Observable macro so swapping it out doesn't force view
    /// re-evaluations.
    @ObservationIgnored
    var nostrActivityDismissTask: Task<Void, Never>?

    /// How long the "Talking to X" capsule stays visible after the latest
    /// turn — matched to win-the-day's 10s window.
    static let nostrActivityIndicatorDuration: TimeInterval = 10

    /// Cold-path application state: settings, subscriptions, podcasts, nostr,
    /// agent, social, threading. These mutate rarely; `didSet` drives
    /// persistence for the cold domains. `episodes` lives outside this struct
    /// (see below) so its high-frequency churn doesn't invalidate cold-screen
    /// observers — `@Observable` tracks per stored property.
    var state: AppState {
        didSet {
            handleStateDidSet()
        }
    }

    /// All known episodes — the hot field. Promoted out of `AppState` into its
    /// own `@Observable` stored property so position-flush / played / download /
    /// triage churn re-renders only episode readers, not settings/nostr/agent
    /// surfaces. This is the runtime source of truth; `AppState.episodes` is a
    /// serialization-only slot, re-composed at the persistence and snapshot
    /// seams (`init`, `clearAllData`, `runStateSideEffects`, kernel projection).
    var episodes: [Episode] = [] {
        didSet {
            handleEpisodesDidSet(previousEpisodes: oldValue)
        }
    }

    /// In-flight on-device model downloads from the unified queue, keyed by
    /// model id. Updated on every download-snapshot tick (alongside the episode
    /// overlay) so the Settings → Providers → Local rows render live progress.
    /// Empty when no model download is active.
    var localModelDownloads: [String: DownloadItemSnapshot] = [:]

    // MARK: - Episode projections (cache)
    //
    // These mirror `state.episodes` so the per-cell O(N) helpers in the
    // Library grid + Home feeds become O(1) dict/Set lookups. See
    // `AppStateStore+EpisodeProjections.swift` for the recompute logic and
    // the read-side adapters that fold the position cache.
    //
    // Stored properties have to live on the class itself (extensions can't
    // add stored state); the methods that build them live in the
    // `+EpisodeProjections` extension.

    /// Unplayed-episode count per subscription. Drives `LibraryGridCell`'s
    /// red dot and the Library "Unplayed" filter chip.
    var unplayedCountByShow: [UUID: Int] = [:]

    /// Subscriptions that have at least one episode in `.downloaded` state.
    /// Drives the Library "Downloaded" filter chip.
    var hasDownloadedByShow: Set<UUID> = []

    /// Subscriptions that have at least one episode with a ready transcript.
    /// Drives the Library "Transcribed" filter chip.
    var hasTranscribedByShow: Set<UUID> = []

    /// Episode array indexes per subscription, pre-sorted newest first.
    /// Drives `ShowDetailView` without duplicating every `Episode` in memory.
    var episodeIndexesByShow: [UUID: [Int]] = [:]

    /// Episodes whose persisted `playbackPosition > 0` and `played == false`,
    /// pre-sorted newest first. Reads merge the position-cache so an episode
    /// the user *just* started (cache > 0, persisted == 0) shows up too.
    var inProgressEpisodesCached: [Episode] = []

    /// Top 30 unplayed episodes across all shows, pre-sorted newest first.
    /// `recentEpisodes(limit:)` returns a prefix of this slice. The fixed
    /// 30 cap matches Home's hard upper bound — anything beyond that the
    /// Home feed never renders, and a smaller cap keeps the cache cheap.
    var recentEpisodesCached: [Episode] = []

    /// Cap used when building `recentEpisodesCached`. Matches Home's
    /// rendered limit; if a caller asks for more we recompute on the fly.
    static let recentEpisodesCacheLimit = 30

    /// Per-show count of *unplayed* episodes the agent triaged into the
    /// inbox (`triageDecision == .inbox && !played`). Backs the inbox-count
    /// roll-up under Home's Inbox header. Played `.inbox` episodes are
    /// excluded — they drop off the surface anyway, so counting them reads
    /// as stale (mirrors the source `triageCounts` semantics).
    var triageInboxCountByShow: [UUID: Int] = [:]

    /// Per-show count of episodes the agent silently archived
    /// (`triageDecision == .archived`). Played state is irrelevant here —
    /// the archived roll-up counts every archived episode regardless.
    var triageArchivedCountByShow: [UUID: Int] = [:]

    /// Shows that have *any* triaged episode (inbox or archived, played or
    /// not). Drives the "across N shows" roll-up. Not derivable from the two
    /// count dicts: a show whose only decided episodes are played `.inbox`
    /// is "covered" yet contributes 0 to both counts, so the covered-show
    /// set is tracked explicitly to reproduce the source `coveredShows.count`.
    var triageDecidedShows: Set<UUID> = []

    /// Storage backing this store. Production code uses `Persistence.shared`
    /// (the App Group suite); tests inject an instance over a unique
    /// in-memory suite so fixtures never leak into the real app.
    let persistence: Persistence

    /// Weak handle to the Rust kernel. Set once by `attachKernel`; used by
    /// mutation methods to dispatch actions without requiring every call site
    /// to hold its own reference.
    @ObservationIgnored
    weak var kernel: KernelModel?

    /// Cancellable observation task that projects `KernelModel` state into
    /// `AppState`. Stored here (not via `objc_setAssociatedObject`) so
    /// `deinit` can cancel it cleanly without a retain cycle.
    @ObservationIgnored
    var kernelObservationTask: Task<Void, Never>?

    /// Observes ONLY `KernelModel.downloadSnapshot` and applies the live
    /// download overlay onto `episodes` row-by-row. Split out of
    /// `kernelObservationTask` so download-progress ticks (which no longer bump
    /// the global kernel `rev` — see `nmp_app_podcast_download_report`) update
    /// just the affected rows instead of re-running the full library
    /// projection / decode / hash pass. Cancelled alongside `kernelObservationTask`.
    @ObservationIgnored
    var downloadOverlayTask: Task<Void, Never>?

    /// Episode IDs from the first kernel snapshot queue, stashed here so
    /// `RootView+Setup` can seed `PlaybackState.queue` even if `attachKernel`
    /// fires before the setup hook is wired. Consumed once and cleared.
    @ObservationIgnored
    var pendingKernelQueue: [UUID] = []

    /// Fires once — when the kernel's initial snapshot arrives — with the
    /// persisted Up Next episode IDs. Wired by `RootView+Setup` to seed
    /// `PlaybackState.queue`. Set to nil after first call.
    @ObservationIgnored
    var onQueueFromKernel: (([UUID]) -> Void)?

    /// Fires on every kernel-projection tick where content changed (position-only
    /// ticks are suppressed by `snapshotContentHash`). Wired in `AppMain` to
    /// `PlatformCapability.applyWidgetSnapshot` so the home-screen widget stays
    /// current from the kernel-owned `WidgetSnapshot` without `PlaybackState`
    /// owning the widget write path. The kernel resolves the widget's
    /// titles/artwork/unplayed-count, so the library is no longer passed.
    @ObservationIgnored
    var onNowPlayingSnapshot: ((PodcastUpdate?) -> Void)?

    /// Fires on every 1 Hz position tick during active playback. Wired in
    /// `AppMain` to `PlatformCapability.applyPositionTick` so the
    /// NowPlayingSnapshot position stays current without `PlaybackState`
    /// owning the throttle counter.
    @ObservationIgnored
    var onPositionTick: ((Double) -> Void)?

    /// Service for loading and executing on-device LLM inference. Registered
    /// with the kernel at attach time so Rust can call back into Swift for
    /// local model responses. Held with app lifetime to keep the
    /// `Unmanaged.passUnretained` FFI context pointer valid.
    @ObservationIgnored
    lazy var localLLMService = LocalLLMService()

    /// Retained observer token for `UIApplication.didEnterBackgroundNotification`.
    /// On background, the position cache is flushed to disk so the user
    /// can force-quit + relaunch without losing playback progress.
    /// See `AppStateStore+PositionDebounce.swift` for the rationale.
    private var backgroundObserver: NSObjectProtocol?

    var mutationBatchDepth = 0
    var deferredStateSideEffects = false
    var deferredEpisodeProjectionRebuild = false
    /// Trailing-debounce task for `WidgetCenter.reloadAllTimelines()`.
    /// Cancelled and re-armed on each mutation so a burst (e.g. marking
    /// 50 episodes played) collapses to a single reload signal — the
    /// system has a daily timeline-reload budget that flooding burns
    /// without producing extra refreshes.
    var widgetReloadTask: Task<Void, Never>?

    /// Echo-suppression flag for iCloud sync. Set by the iCloud capability
    /// while applying a remote change so `updateSettings` does not re-dispatch
    /// the same values back to the kernel (breaking the one-way sync).
    var isApplyingRemoteChange: Bool = false

    // MARK: - Position debounce
    //
    // Position updates from `PlaybackState.tickPersistence` arrive at 1 Hz.
    // Writing the entire ~8 MB JSON blob every second would be 480 MB/min of
    // disk I/O on the main actor — battery, NAND wear, and main-thread
    // responsiveness all suffer. We coalesce position updates through these
    // three fields and only mutate `state.episodes` (which would trigger the
    // expensive save) on a controlled cadence.
    //
    // See `AppStateStore+PositionDebounce.swift` for the full read/write
    // contract; these properties are declared here because they're stored
    // properties (extensions can't add stored state) and isolated to the
    // store's main actor.

    /// Cached playback positions waiting to be folded into `state.episodes`.
    /// Read-folded into `episode(id:)`/`inProgressEpisodes`/`recentEpisodes`
    /// so UI surfaces never see a stale position. Drained by
    /// `flushPendingPositions()`.
    var positionCache: [UUID: TimeInterval] = [:]

    /// Pending trailing-debounce flush task. Cancelled and re-armed on each
    /// `setEpisodePlaybackPosition` call so the deadline keeps moving while
    /// updates stream in (true trailing debounce).
    var positionFlushTask: Task<Void, Never>?

    /// Wall-clock time of the most recent position flush. Drives the
    /// max-interval cap: if continuous updates exceed
    /// `positionMaxInterval` since this timestamp, the next call writes
    /// eagerly so a crash never loses more than one cap-window of
    /// position.
    var lastPositionFlush: Date?

    init(persistence: Persistence = .shared) {
        self.persistence = persistence
        var loadedState: AppState
        do {
            loadedState = try persistence.load()
        } catch {
            Self.logger.error("Persistence.load failed: \(error, privacy: .public) — starting with empty state")
            loadedState = AppState()
        }
        // Strip legacy external-playback podcasts written by an earlier
        // build that used an `external-episode://` sentinel feed URL. The
        // new model parents external episodes to `Podcast.unknownID` (or a
        // real podcast row when a feed_url is supplied), so these legacy
        // artifacts should not appear in the library.
        let legacyExternalPodcastIDs = Set(
            loadedState.podcasts
                .filter { $0.feedURL?.scheme == "external-episode" }
                .map(\.id)
        )
        if !legacyExternalPodcastIDs.isEmpty {
            loadedState.podcasts.removeAll { legacyExternalPodcastIDs.contains($0.id) }
            loadedState.subscriptions.removeAll { legacyExternalPodcastIDs.contains($0.podcastID) }
        }
        // Split the freshly-loaded episodes out of the DTO into the dedicated
        // `episodes` stored property — the runtime source of truth. We blank
        // `state.episodes` so there is exactly one live copy in memory; the
        // save seam (`runStateSideEffects`) re-composes the DTO on the way to
        // disk. Both assignments here are the first write to their respective
        // stored properties, so neither `didSet` fires during init.
        self.episodes = loadedState.episodes
        loadedState.episodes = []
        self.state = loadedState
        // The `didSet`s above don't fire from inside `init` until all stored
        // properties are initialised, and even then they skip the very first
        // assignment in init. Build the projections by hand from the freshly-
        // loaded episodes so the first SwiftUI render after launch already
        // sees populated caches — otherwise the Library grid would briefly
        // read empty unplayed dots until the first mutation.
        recomputeEpisodeProjections()
        // Bootstrap the live RAG stack so the SQLite vector store is opened
        // (and its file path logged) before any view tries to query it.
        // Hand `self` to the service so the retrieval adapters and transcript
        // ingester can resolve episode/subscription metadata.
        RAGService.shared.attach(appStore: self)
        // Prune agent-activity entries older than 30 days so the persisted log
        // doesn't grow unboundedly across many months of use. This fires one
        // Persistence.save only when stale entries are actually found.
        pruneStaleActivityEntries()
        // Spotlight indexing is disabled — the formatter pass over hundreds of
        // multi-KB show-notes blobs was monopolizing a cooperative worker for
        // tens of seconds on every state change. Clear anything we previously
        // published so the app doesn't continue to litter the system index
        // with stale entries that no longer get refreshed.
        SpotlightIndexer.clearAll()
        // Feed refresh is driven by the Rust kernel (lifecycle foreground
        // triggers `refresh_all`). The legacy Swift refresh loop is skipped
        // when `kernel` is non-nil (set by `attachKernel`). We start it here
        // Subscribe to app-backgrounding so the position cache is flushed
        // to disk before iOS can suspend or kill the process. Token is
        // retained on `self` so the observer outlives the init call but
        // dies with the store. See `AppStateStore+PositionDebounce.swift`.
        backgroundObserver = registerBackgroundFlushObserver()
    }

    /// Wipes all user data while preserving API credentials and Nostr identity.
    func clearAllData() {
        // Drop any queued position writes — they would target episode IDs
        // about to disappear and could resurrect deleted records on the
        // next flush.
        positionFlushTask?.cancel()
        positionFlushTask = nil
        widgetReloadTask?.cancel()
        widgetReloadTask = nil
        positionCache.removeAll()

        let preserved = state.settings
        performMutationBatch {
            state = AppState()
            state.settings = preserved
            // Episodes now live in their own stored property — `state = AppState()`
            // no longer zeroes them, so wipe them explicitly. The `episodes.didSet`
            // fingerprint catches the N→0 count change and rebuilds the projections
            // to empty; the explicit `invalidateEpisodeProjections()` is belt-and-
            // suspenders against future refactors that might bypass didSet.
            episodes = []
            invalidateEpisodeProjections()
        }
        SpotlightIndexer.clearAll()
    }

    deinit {
        // NotificationCenter retains observer tokens until they're removed,
        // even after the registering instance dies. Without this, the
        // closure would keep firing into a `nil` self (harmless but noisy)
        // and the test target would leak observers across runs.
        //
        // Swift 6 deinit is nonisolated; we can't touch the @MainActor
        // stored properties from here directly. The observer tokens and
        // Task we need to clean up are conceptually owned by the actor,
        // but `removeObserver` is thread-safe and `Task.cancel()` is
        // `Sendable`, so we can safely reach them via `assumeIsolated` —
        // by the time deinit runs, no other actor work can be racing
        // against us for `self`.
        MainActor.assumeIsolated {
            if let backgroundObserver {
                NotificationCenter.default.removeObserver(backgroundObserver)
            }
            kernelObservationTask?.cancel()
            downloadOverlayTask?.cancel()
            positionFlushTask?.cancel()
            widgetReloadTask?.cancel()
        }
    }
}
