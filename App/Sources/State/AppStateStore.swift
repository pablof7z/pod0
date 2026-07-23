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
    @ObservationIgnored private(set) var startupRecoveryRequired = false
    var recallConfigurationRevision: UInt64 = 0
    var transcriptReader: any TranscriptReading {
        sharedLibrary?.authoritativeTranscriptReader ?? UnavailableTranscriptReader.shared
    }
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
            handleStateDidSet(previousEpisodes: oldValue.episodes)
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

    func mutateState(ensuring jobs: [DesiredJob], _ mutation: (inout AppState) -> Void) {
        guard !startupRecoveryRequired else {
            Self.logger.error("Blocked native job mutation while startup recovery is required")
            return
        }
        pendingAtomicJobs.append(contentsOf: jobs)
        mutateState(mutation)
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

    /// Episode array indexes per subscription, pre-sorted newest first.
    /// Drives `ShowDetailView` without duplicating every `Episode` in memory.
    var episodeIndexesByShow: [UUID: [Int]] = [:]

    /// Episodes whose Rust-projected `playbackPosition > 0` and `played == false`,
    /// pre-sorted newest first.
    var inProgressEpisodesCached: [Episode] = []

    /// Top 30 unplayed episodes across all shows, pre-sorted newest first.
    /// `recentEpisodes(limit:)` returns a prefix of this slice. The fixed
    /// 30 cap matches Home's hard upper bound — anything beyond that the
    /// Home feed never renders, and a smaller cap keeps the cache cheap.
    var recentEpisodesCached: [Episode] = []

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
    var deferredStateSideEffects = false
    var pendingAtomicJobs: [DesiredJob] = []
    var deferredEpisodeProjectionRebuild = false
    /// Trailing-debounce task for `WidgetCenter.reloadAllTimelines()`.
    /// Cancelled and re-armed on each mutation so a burst (e.g. marking
    /// 50 episodes played) collapses to a single reload signal — the
    /// system has a daily timeline-reload budget that flooding burns
    /// without producing extra refreshes.
    var widgetReloadTask: Task<Void, Never>?

    init(
        persistence: Persistence = .shared,
        productSignals: any ProductSignalSink = DiscardingProductSignalSink.shared,
        sharedFeedHost: (any CoreFeedHosting)? = nil,
        startSubscriptionRefresh: Bool = true
    ) {
        self.persistence = persistence
        syncSettingsWithICloud = persistence === Persistence.shared
        self.productSignals = productSignals
        var loadedState: AppState
        var startupLoadFailed = false
        do {
            let chapterAuthorityActive = FileManager.default.fileExists(
                atPath: persistence.sharedCoreStoreURL.path
            ) && sharedChapterStoreIsAuthoritative(
                targetPath: persistence.sharedCoreStoreURL.path
            )
            loadedState = try persistence.load(
                loadLegacyChapterAdjuncts: !chapterAuthorityActive
            )
        } catch {
            Self.logger.error(
                "Persistence.load failed; startup is blocked and persisted data is untouched"
            )
            startupLoadFailed = true
            loadedState = AppState()
        }
        if startupLoadFailed {
            self.state = loadedState
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
        Self.migrateLegacyOpenRouterSecretIfNeeded(in: &loadedState, persistence: persistence)
        // Strip synthetic external-playback podcasts written by an earlier
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
        if !FileManager.default.fileExists(atPath: persistence.sharedCoreStoreURL.path) {
            let nextGeneration = loadedState.persistenceGeneration == .max
                ? UInt64.max
                : loadedState.persistenceGeneration + 1
            let importRevision = max(nextGeneration, 1)
            loadedState.persistenceGeneration = importRevision
            _ = persistence.write(loadedState, revision: importRevision)
        }
        // Start iCloud KV sync before assigning state so that the first
        // push (triggered by the `didSet` below) reflects the merged values.
        if syncSettingsWithICloud {
            iCloudSettingsSync.shared.start(mergingInto: &loadedState.settings)
        }
        self.state = loadedState
        let feedHost: any CoreFeedHosting = sharedFeedHost ?? CoreFeedHost()
        switch SharedLibraryBootstrap.run(
            persistence: persistence,
            legacyState: loadedState,
            feedHost: feedHost,
            chapterCompilationModel: loadedState.settings.chapterCompilationModel,
            legacyRecallConfiguration: loadedState.settings.legacyRecallConfigurationSeed
        ) {
        case .ready(let client):
            sharedLibrary = client
            client.attach(store: self)
            if state.settings.legacyRecallConfigurationSeed != nil {
                mutateState { $0.settings.retireLegacyRecallConfiguration() }
                if syncSettingsWithICloud {
                    iCloudSettingsSync.shared.retireLegacyRecallConfiguration()
                }
            }
        case .authoritativeUnavailable(let reason, _):
            sharedLibraryUnavailableReason = reason
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
        // Prune agent-activity entries older than 30 days so the persisted log
        // doesn't grow unboundedly across many months of use. This fires one
        // Persistence.save only when stale entries are actually found.
        pruneStaleActivityEntries()
        // One-time cleanup of the deleted Wiki feature's on-disk pages
        // (`Application Support/podcastr/wiki/`). Guarded by a UserDefaults
        // flag so this touches the filesystem at most once per install.
        Self.cleanupOrphanedWikiFilesIfNeeded()
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
