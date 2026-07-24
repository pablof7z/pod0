import Foundation
import Observation
import Pod0Core
import WidgetKit
import os.log

/// Native projection and temporary-domain store.
///
/// Rust is the sole durable owner of the migrated listening slice. This store
/// persists unmigrated product domains and a replaceable native read model.
@MainActor
@Observable
final class AppStateStore {
    nonisolated static let logger = Logger.app("AppStateStore")
    let productSignals: any ProductSignalSink
    @ObservationIgnored private(set) var sharedLibrary: SharedLibraryClient?
    @ObservationIgnored private(set) var sharedLibraryUnavailableReason: String?
    @ObservationIgnored private(set) var sharedLibraryUnavailableStage: SharedLibraryBootstrapStage?
    @ObservationIgnored private(set) var startupRecoveryRequired = false
    /// Bounded Rust projection; never persisted as native durable state.
    var newEpisodeNotificationsEnabled = true
    var recallConfigurationRevision: UInt64 = 0
    /// Chapter the user long-pressed in `PlayerChaptersScrollView`. Drained
    /// by `SharedAgentChatView` and prefilled into the composer; cleared by
    /// the same presentation so a later sheet re-open starts blank. Carries no
    /// transcript text — only the chapter title + time range; the agent
    /// fetches transcript context through its tool inventory.
    var pendingChapterAgentContext: ChapterAgentContext?
    /// Voice note the user recorded via the mic button in the player. Drained
    /// by `SharedAgentChatView` and auto-sent to the agent. The context
    /// carries the timestamp anchor, the active chapter bounds, and the
    /// transcribed utterance; the agent decides what to do with it.
    var pendingVoiceNoteAgentContext: VoiceNoteAgentContext?
    private(set) var state: AppState {
        didSet {
            handleStateDidSet(previousState: oldValue)
        }
    }
    /// The only write gate for companion store extensions and test fixtures.
    func mutateState(_ mutation: (inout AppState) -> Void) {
        guard !startupRecoveryRequired else {
            Self.logger.error("Blocked native state mutation while startup recovery is required")
            return
        }
        var updated = state
        mutation(&updated)
        state = updated
    }

    /// Replaces a bounded Rust projection without turning the native read
    /// model into a second durable writer.
    func mutateProjectionState(_ mutation: (inout AppState) -> Void) {
        guard !startupRecoveryRequired else {
            Self.logger.error("Blocked native projection mutation while startup recovery is required")
            return
        }
        projectionMutationDepth += 1
        defer { projectionMutationDepth -= 1 }
        var updated = state
        mutation(&updated)
        state = updated
    }

    // MARK: - Episode projections (cache)
    //
    // These mirror `state.episodes` so the per-cell O(N) helpers in the
    // Library grid + Home feeds become O(1) dict/Set lookups. See
    // `AppStateStore+EpisodeProjections.swift` for the recompute logic and
    // the read-side adapters that materialize bounded native projections.
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
    /// Read-model indexes keep playback UI lookups independent of library size.
    var episodeIndexByID: [UUID: Int] = [:]
    var podcastIndexByID: [UUID: Int] = [:]
    var subscriptionIndexByPodcastID: [UUID: Int] = [:]

    /// Episode indexes per subscription, pre-sorted newest first.
    var episodeIndexesByShow: [UUID: [Int]] = [:]
    var allEpisodeIndexesNewestFirst: [Int] = []

    /// Episodes whose Rust-projected `playbackPosition > 0` and `played == false`,
    /// pre-sorted newest first.
    var inProgressEpisodesCached: [Episode] = []

    /// Top 30 unplayed episodes across all shows, pre-sorted newest first.
    /// `recentEpisodes(limit:)` returns a prefix of this slice. The fixed
    /// 30 cap matches Home's hard upper bound — anything beyond that the
    /// Home feed never renders, and a smaller cap keeps the cache cheap.
    var recentEpisodesCached: [Episode] = []
    /// Indexes for the small subsets used by download and Saved surfaces.
    /// Keeping these alongside the other episode projections prevents those
    /// screens from rescanning the whole library during progress updates.
    var downloadedEpisodeIndexes: [Int] = []
    var starredEpisodeIndexes: [Int] = []

    /// Cap used when building `recentEpisodesCached`. Matches Home's
    /// rendered limit; if a caller asks for more we recompute on the fly.
    static let recentEpisodesCacheLimit = 30

    /// Storage backing this store. Production code uses `Persistence.shared`
    /// (the App Group suite); tests inject an instance over a unique
    /// in-memory suite so fixtures never leak into the real app.
    let persistence: Persistence
    /// Only the production App Group store participates in the process-wide
    /// iCloud settings channel. Injected stores are isolated test or preview
    /// fixtures and must neither import nor publish account-wide preferences.
    let syncSettingsWithICloud: Bool

    /// Retained observer token for iCloud external-change notifications.
    private var iCloudObserver: NSObjectProtocol?

    var mutationBatchDepth = 0
    /// Non-zero while Rust-owned durable state is replacing a native read
    /// model. Projection updates rebuild derived caches but must not flow back
    /// into Swift persistence, iCloud, or widget side effects.
    var projectionMutationDepth = 0
    var deferredStateSideEffects = false
    var pendingAtomicJobs: [DesiredJob] = []
    var deferredEpisodeProjectionRebuild = false
    /// Trailing-debounce task for `WidgetCenter.reloadAllTimelines()`.
    /// Cancelled and re-armed on each mutation so a burst (e.g. marking
    /// 50 episodes played) collapses to a single reload signal — the
    /// system has a daily timeline-reload budget that flooding burns
    /// without producing extra refreshes.
    var widgetReloadTask: Task<Void, Never>?

    convenience init(
        persistence: Persistence = .shared,
        productSignals: any ProductSignalSink = DiscardingProductSignalSink.shared,
        sharedFeedHost: (any CoreFeedHosting)? = nil,
        startSubscriptionRefresh: Bool = true
    ) {
        self.init(
            preparedStartup: AppStateStartupPreparer.prepare(
                persistence: persistence,
                sharedFeedHost: sharedFeedHost
            ),
            persistence: persistence,
            productSignals: productSignals,
            startSubscriptionRefresh: startSubscriptionRefresh
        )
        if persistence !== Persistence.shared {
            sharedLibrary?.hydrateSynchronouslyForTesting()
        }
    }

    init(
        preparedStartup: AppStateStartupPreparation,
        persistence: Persistence,
        productSignals: any ProductSignalSink,
        startSubscriptionRefresh: Bool
    ) {
        self.persistence = persistence
        syncSettingsWithICloud = persistence === Persistence.shared
        self.productSignals = productSignals
        var loadedState = preparedStartup.state
        if syncSettingsWithICloud, !preparedStartup.loadFailed {
            iCloudSettingsSync.shared.start(mergingInto: &loadedState.settings)
        }
        self.state = loadedState
        if preparedStartup.loadFailed {
            startupRecoveryRequired = true
            sharedLibraryUnavailableReason = "app_state_recovery_required"
            Task {
                await productSignals.record(.init(
                    name: .dataLossEvidence,
                    outcome: .detected,
                    errorClass: .corruptArtifact
                ))
            }
            recomputeEpisodeProjections()
            return
        }
        let bootstrap = preparedStartup.bootstrap
            ?? .authoritativeUnavailable(reason: "bootstrap_missing", stage: .storePreparation)
        switch SharedLibraryBootstrap.finish(bootstrap, persistence: persistence) {
        case .ready(let client):
            sharedLibrary = client
            client.attach(store: self)
            if preparedStartup.needsRecallConfigurationRetirement {
                mutateState { $0.settings.retireLegacyRecallConfiguration() }
                if syncSettingsWithICloud {
                    iCloudSettingsSync.shared.retireLegacyRecallConfiguration()
                }
            } else if preparedStartup.needsNativeProjectionRetirement {
                let retirementState = state
                persistence.commitStartupRetirement(retirementState)
            }
        case .authoritativeUnavailable(let reason, let stage):
            sharedLibraryUnavailableReason = reason
            sharedLibraryUnavailableStage = stage
        }
        // The `state.didSet` above doesn't fire from inside `init` until all
        // stored properties are initialised, and even then it skips the very
        // first assignment in init. Build the projections by hand from the
        // freshly-loaded state so the first SwiftUI render after launch
        // already sees populated caches — otherwise the Library grid would
        // briefly read empty unplayed dots until the first mutation.
        recomputeEpisodeProjections()
        // Fail closed before any service can mutate or persist the legacy
        // migration source. A later launch resumes from the verified evidence
        // after the core is repaired; Swift never becomes fallback authority.
        guard sharedLibrary != nil else { return }
        // Attach the native capability executor used by the Rust recall workflow.
        // Rust supplies the exact provider, model, and dimensionality.
        sharedLibrary?.attachRecall(RecallProviderService.shared, store: self)
        WorkflowRuntime.shared.attach(store: self)
        BackgroundWorkScheduler.shared.attach(store: self)
        Task.detached(priority: .utility) {
            Self.cleanupOrphanedWikiFilesIfNeeded()
        }
        // Spotlight indexing is disabled — the formatter pass over hundreds of
        // multi-KB show-notes blobs was monopolizing a cooperative worker for
        // tens of seconds on every state change. Clear anything we previously
        // published so the app doesn't continue to litter the system index
        // with stale entries that no longer get refreshed.
        SpotlightIndexer.clearAll()
        // Observe external iCloud changes so settings stay in sync while the
        // app is running on multiple devices simultaneously.
        if syncSettingsWithICloud {
            iCloudObserver = NotificationCenter.default.addObserver(
                forName: iCloudSettingsSync.settingsDidChangeExternallyNotification,
                object: nil,
                queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated {
                    self?.applyExternalSettingsChange()
                }
            }
        }
        // Refresh once for this foreground lifecycle. Later opportunities are
        // delivered by foreground notifications and BGTaskScheduler.
        if startSubscriptionRefresh {
            SubscriptionRefreshService.shared.startLifecycleRefresh(store: self)
        }
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
            if let iCloudObserver {
                NotificationCenter.default.removeObserver(iCloudObserver)
            }
            widgetReloadTask?.cancel()
            sharedLibrary?.shutdown()
        }
    }
}
