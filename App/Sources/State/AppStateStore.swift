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

    var state: AppState {
        didSet {
            handleStateDidSet(previousEpisodes: oldValue.episodes)
        }
    }

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
    /// `PlatformCapability.applyNowPlayingSnapshot` so the widget stays current
    /// without `PlaybackState` owning the widget write path.
    @ObservationIgnored
    var onNowPlayingSnapshot: ((PodcastUpdate?, [PodcastSummary]) -> Void)?

    /// Retained observer token for iCloud external-change notifications.
    private var iCloudObserver: NSObjectProtocol?

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
        // Keychain migration: move legacy per-service keys into PcstIdentityCapability
        // slots. v2 adds the Ollama slot; re-running is safe (skip-if-present guard).
        LegacyKeychainMigration.runIfNeeded()
        // JSON→Keychain pump: older builds stored the OpenRouter key as plaintext in
        // the AppState JSON blob. The Keychain migrator cannot read that — it only
        // knows about previous Keychain services. This pump handles that upgrade path.
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
        // Start iCloud KV sync before assigning state so that the first
        // push (triggered by the `didSet` below) reflects the merged values.
        iCloudSettingsSync.shared.start(mergingInto: &loadedState.settings)
        self.state = loadedState
        // The `state.didSet` above doesn't fire from inside `init` until all
        // stored properties are initialised, and even then it skips the very
        // first assignment in init. Build the projections by hand from the
        // freshly-loaded state so the first SwiftUI render after launch
        // already sees populated caches — otherwise the Library grid would
        // briefly read empty unplayed dots until the first mutation.
        recomputeEpisodeProjections()
        // Bootstrap the live RAG stack so the SQLite vector store is opened
        // (and its file path logged) before any view tries to query it.
        // Hand `self` to the service so the briefing adapter and transcript
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
        // Observe external iCloud changes so settings stay in sync while the
        // app is running on multiple devices simultaneously.
        iCloudObserver = NotificationCenter.default.addObserver(
            forName: iCloudSettingsSync.settingsDidChangeExternallyNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                self?.applyExternalSettingsChange()
            }
        }
        // Feed refresh is driven by the Rust kernel (lifecycle foreground
        // triggers `refresh_all`). The legacy Swift refresh loop is skipped
        // when `kernel` is non-nil (set by `attachKernel`). We start it here
        // Subscribe to app-backgrounding so the position cache is flushed
        // to disk before iOS can suspend or kill the process. Token is
        // retained on `self` so the observer outlives the init call but
        // dies with the store. See `AppStateStore+PositionDebounce.swift`.
        backgroundObserver = registerBackgroundFlushObserver()
    }

    /// Pulls the latest iCloud values into `state.settings`.
    /// Called when `iCloudSettingsSync` reports an external change.
    private func applyExternalSettingsChange() {
        let sync = iCloudSettingsSync.shared
        sync.isApplyingRemoteChange = true
        defer { sync.isApplyingRemoteChange = false }
        var updated = state.settings
        sync.merge(from: NSUbiquitousKeyValueStore.default, into: &updated)
        guard updated != state.settings else { return }
        Self.logger.info("iCloudSettingsSync: applying remote settings update")
        // Assign directly (bypassing updateSettings) to avoid a redundant push.
        state.settings = updated
    }

    // MARK: - Legacy migration helpers

    /// Migrate the plaintext OpenRouter key from the AppState JSON blob into the
    /// Keychain (PcstIdentityCapability.byokOpenRouter). Older builds stored the
    /// key in `Settings.legacyOpenRouterAPIKey`; LegacyKeychainMigration cannot
    /// reach JSON fields — only previous Keychain services — so this pump is still
    /// needed for users upgrading from JSON-blob storage.
    private static func migrateLegacyOpenRouterSecretIfNeeded(
        in state: inout AppState,
        persistence: Persistence
    ) {
        let legacyKey = state.settings.legacyOpenRouterAPIKey?.trimmingCharacters(in: .whitespaces) ?? ""
        guard !legacyKey.isEmpty else {
            state.settings.legacyOpenRouterAPIKey = nil
            return
        }
        do {
            try OpenRouterCredentialStore.saveAPIKey(legacyKey)
            state.settings.markOpenRouterManual()
        } catch {
            logger.error("Failed to migrate legacy OpenRouter key to keychain: \(error, privacy: .public)")
            state.settings.clearOpenRouterCredential()
        }
        state.settings.legacyOpenRouterAPIKey = nil
        persistence.save(state)
    }

    // MARK: - Settings

    func updateSettings(_ settings: Settings) {
        let prior = state.settings
        state.settings = settings
        // Mirror the Rust-owned subset of settings to the kernel so they
        // survive across restarts (Rust persists them in podcasts.json).
        if settings.autoSkipAds != prior.autoSkipAds {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: ["op": "set_auto_skip_ads", "enabled": settings.autoSkipAds])
        }
        if settings.skipForwardSeconds != prior.skipForwardSeconds
            || settings.skipBackwardSeconds != prior.skipBackwardSeconds {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: [
                                 "op": "set_skip_intervals",
                                 "forward_secs": Double(settings.skipForwardSeconds),
                                 "backward_secs": Double(settings.skipBackwardSeconds)
                             ])
        }
        if settings.autoPlayNext != prior.autoPlayNext {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: ["op": "set_auto_play_next", "enabled": settings.autoPlayNext])
        }
        if settings.autoMarkPlayedAtEnd != prior.autoMarkPlayedAtEnd {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: ["op": "set_auto_mark_played_at_end", "enabled": settings.autoMarkPlayedAtEnd])
        }
        if settings.headphoneDoubleTapAction != prior.headphoneDoubleTapAction
            || settings.headphoneTripleTapAction != prior.headphoneTripleTapAction {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: [
                                 "op": "set_headphone_gesture_actions",
                                 "double_tap": settings.headphoneDoubleTapAction.rawValue,
                                 "triple_tap": settings.headphoneTripleTapAction.rawValue
                             ])
        }
        if settings.hasCompletedOnboarding != prior.hasCompletedOnboarding {
            kernel?.dispatch(namespace: "podcast",
                             body: [
                                 "op": "update_settings",
                                 "has_completed_onboarding": settings.hasCompletedOnboarding
                             ])
        }
        if settings.defaultPlaybackRate != prior.defaultPlaybackRate {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: ["op": "set_default_playback_rate", "rate": settings.defaultPlaybackRate])
        }
        if settings.autoDeleteDownloadsAfterPlayed != prior.autoDeleteDownloadsAfterPlayed {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: ["op": "set_auto_delete_downloads_after_played",
                                    "enabled": settings.autoDeleteDownloadsAfterPlayed])
        }
        if settings.agentInitialModel != prior.agentInitialModel
            || settings.agentInitialModelName != prior.agentInitialModelName {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: [
                                 "op": "set_agent_initial_model",
                                 "model": settings.agentInitialModel,
                                 "model_name": settings.agentInitialModelName
                             ])
        }
        if settings.agentThinkingModel != prior.agentThinkingModel
            || settings.agentThinkingModelName != prior.agentThinkingModelName {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: [
                                 "op": "set_agent_thinking_model",
                                 "model": settings.agentThinkingModel,
                                 "model_name": settings.agentThinkingModelName
                             ])
        }
        if settings.memoryCompilationModel != prior.memoryCompilationModel
            || settings.memoryCompilationModelName != prior.memoryCompilationModelName {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: [
                                 "op": "set_memory_compilation_model",
                                 "model": settings.memoryCompilationModel,
                                 "model_name": settings.memoryCompilationModelName
                             ])
        }
        if settings.wikiModel != prior.wikiModel
            || settings.wikiModelName != prior.wikiModelName {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: [
                                 "op": "set_wiki_model",
                                 "model": settings.wikiModel,
                                 "model_name": settings.wikiModelName
                             ])
        }
        if settings.categorizationModel != prior.categorizationModel
            || settings.categorizationModelName != prior.categorizationModelName {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: [
                                 "op": "set_categorization_model",
                                 "model": settings.categorizationModel,
                                 "model_name": settings.categorizationModelName
                             ])
        }
        if settings.chapterCompilationModel != prior.chapterCompilationModel
            || settings.chapterCompilationModelName != prior.chapterCompilationModelName {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: [
                                 "op": "set_chapter_compilation_model",
                                 "model": settings.chapterCompilationModel,
                                 "model_name": settings.chapterCompilationModelName
                             ])
        }
        if settings.embeddingsModel != prior.embeddingsModel
            || settings.embeddingsModelName != prior.embeddingsModelName {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: [
                                 "op": "set_embeddings_model",
                                 "model": settings.embeddingsModel,
                                 "model_name": settings.embeddingsModelName
                             ])
        }
        if settings.imageGenerationModel != prior.imageGenerationModel
            || settings.imageGenerationModelName != prior.imageGenerationModelName {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: [
                                 "op": "set_image_generation_model",
                                 "model": settings.imageGenerationModel,
                                 "model_name": settings.imageGenerationModelName
                             ])
        }
        if settings.rerankerEnabled != prior.rerankerEnabled {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: ["op": "set_reranker_enabled", "enabled": settings.rerankerEnabled])
        }
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
            // `state = AppState()` above changes the episode array's count from
            // N to 0, so the `state.didSet` fingerprint catches it and rebuilds
            // the projections to empty. Explicit call here is belt-and-
            // suspenders against future refactors that might bypass didSet.
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
            if let iCloudObserver {
                NotificationCenter.default.removeObserver(iCloudObserver)
            }
            kernelObservationTask?.cancel()
            positionFlushTask?.cancel()
            widgetReloadTask?.cancel()
        }
    }
}
