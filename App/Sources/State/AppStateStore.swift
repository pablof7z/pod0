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

    /// Rust-owned cross-episode threading projection. The kernel derives this
    /// from library/transcript/category facts; Swift keeps it transient and
    /// renders native rows only.
    var threadingProjection: ThreadingProjection = .empty

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

    /// Queue items from the first kernel snapshot queue, stashed here so
    /// `RootView+Setup` can seed `PlaybackState.queue` even if `attachKernel`
    /// fires before the setup hook is wired. Consumed once and cleared.
    @ObservationIgnored
    var pendingKernelQueue: [QueueItem] = []

    /// Fires once — when the kernel's initial snapshot arrives — with the
    /// persisted Up Next queue items. Wired by `RootView+Setup` to seed
    /// `PlaybackState.queue`. Set to nil after first call.
    @ObservationIgnored
    var onQueueFromKernel: (([QueueItem]) -> Void)?

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

    init(
        persistence: Persistence = .shared
    ) {
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
        // Historical compatibility hook. Episode-derived product projections
        // are Rust-owned now, but mutation/init paths still call this no-op
        // while the surrounding store lifecycle is being simplified.
        recomputeEpisodeProjections()
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
        // Wire the transcript ingest service so it can reach this store.
        // RAGService.shared.attach was the previous host for this wiring;
        // slice 5f retired RAGService, so TranscriptIngestService owns it now.
        TranscriptIngestService.shared.attach(appStore: self)
    }

    /// Wipes all user data while preserving API credentials and Nostr identity.
    func clearAllData() {
        widgetReloadTask?.cancel()
        widgetReloadTask = nil

        let preserved = state.settings
        performMutationBatch {
            state = AppState()
            state.settings = preserved
            kernel?.dispatch(namespace: "podcast.memory",
                             body: ["op": "forget_all"])
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
        // Swift 6 deinit is nonisolated; we can't touch the @MainActor
        // stored properties from here directly. Task.cancel() is `Sendable`,
        // so we can safely reach them via `assumeIsolated` — by the time
        // deinit runs, no other actor work can be racing against us for `self`.
        MainActor.assumeIsolated {
            kernelObservationTask?.cancel()
            downloadOverlayTask?.cancel()
            widgetReloadTask?.cancel()
        }
    }
}
