import Foundation
import os.log

// MARK: - LivePodcastAgentToolDeps
//
// Wires the lane-10 podcast tool surface (`AgentTools.dispatchPodcast`) to the
// real services that ship in the app:
//
//   • `PodcastAgentRAGSearchProtocol`  → `LivePodcastRAGAdapter`
//   • `WikiStorageProtocol`            → `LiveWikiStorageAdapter`
//   • `BriefingComposerProtocol`       → `LiveBriefingComposerAdapter`
//   • `EpisodeSummarizerProtocol`      → `LiveEpisodeSummarizerAdapter`
//   • `EpisodeFetcherProtocol`         → `LiveEpisodeFetcherAdapter`
//   • `PlaybackHostProtocol`           → `LivePlaybackHostAdapter`
//   • `PeerEventPublisherProtocol`     → `LivePeerEventPublisher`
//   • `PerplexityClientProtocol`       → `PerplexityClient`
//   • `TTSPublisherProtocol`           → `AgentTTSComposer`
//
// Constructed once per `AgentChatSession` / `AgentRelayBridge`, the bundle
// holds weak references to `AppStateStore` and `PlaybackState` so the agent
// adapters never extend their lifetimes. Heavy adapters (RAG, Briefing,
// Summarizer) live in their own files; the small ones live here.

@MainActor
enum LivePodcastAgentToolDeps {

    static let logger = Logger.app("AgentTools")

    /// Build a `PodcastAgentToolDeps` bundle wired to the live services.
    /// Call once at session construction; pass the result through to
    /// `AgentTools.dispatchPodcast(...)` for every tool call.
    static func make(
        store: AppStateStore,
        playback: PlaybackState
    ) -> PodcastAgentToolDeps {
        let inventory = LivePodcastInventoryAdapter(store: store)
        return PodcastAgentToolDeps(
            rag: LivePodcastRAGAdapter(store: store),
            wiki: LiveWikiStorageAdapter(store: store),
            briefing: LiveBriefingComposerAdapter(store: store),
            summarizer: LiveEpisodeSummarizerAdapter(store: store),
            fetcher: LiveEpisodeFetcherAdapter(store: store),
            playback: LivePlaybackHostAdapter(store: store, playback: playback),
            library: LivePodcastLibraryAdapter(
                store: store,
                downloadService: .shared,
                transcriptService: .shared,
                refreshService: .shared
            ),
            inventory: inventory,
            categories: inventory,
            peerPublisher: LivePeerEventPublisher(store: store),
            friendDirectory: LiveFriendDirectoryAdapter(store: store),
            pendingRegistrar: LivePendingFriendMessageRegistrar(store: store),
            perplexity: PerplexityClient(),
            ttsPublisher: AgentTTSComposer(store: store, playback: playback),
            directory: LivePodcastDirectoryAdapter(),
            subscribe: LivePodcastSubscribeAdapter(store: store),
            youtubeIngestion: LiveYouTubeIngestionAdapter(store: store),
            ownedPodcasts: LiveAgentOwnedPodcastManager(store: store),
            peerContext: nil
        )
    }
}

// MARK: - Fetcher adapter

/// Resolves episode existence + display metadata from the in-memory
/// `AppStateStore`. Fast — every lookup is a linear scan over the episode
/// array, but the array is bounded by user subscriptions so this is fine.
struct LiveEpisodeFetcherAdapter: EpisodeFetcherProtocol {

    weak var store: AppStateStore?

    init(store: AppStateStore) {
        self.store = store
    }

    func episodeExists(episodeID: EpisodeID) async -> Bool {
        guard let uuid = UUID(uuidString: episodeID) else { return false }
        return await store?.episode(id: uuid) != nil
    }

    func episodeMetadata(
        episodeID: EpisodeID
    ) async -> (podcastTitle: String, episodeTitle: String, durationSeconds: Int?)? {
        guard let store, let uuid = UUID(uuidString: episodeID),
              let episode = await store.episode(id: uuid) else { return nil }
        let podcast = await store.podcast(id: episode.podcastID)
        return (
            podcastTitle: podcast?.title ?? "",
            episodeTitle: episode.title,
            durationSeconds: episode.duration.map { Int($0) }
        )
    }

    func episodeIDForAudioURL(_ audioURLString: String, podcastID: PodcastID) async -> EpisodeID? {
        guard let store, let podcastUUID = UUID(uuidString: podcastID) else { return nil }
        let episodes = await store.episodes(forPodcast: podcastUUID)
        return episodes.first { $0.enclosureURL.absoluteString == audioURLString }?.id.uuidString
    }
}

// MARK: - Playback adapter

/// Drives the live `PlaybackState` from agent tool calls. Uses weak refs so
/// the agent surface never extends the player's lifetime past the SwiftUI
/// scene that owns it.
final class LivePlaybackHostAdapter: PlaybackHostProtocol, @unchecked Sendable {

    private let logger = Logger.app("AgentTools")
    weak var store: AppStateStore?
    weak var playback: PlaybackState?

    init(store: AppStateStore, playback: PlaybackState) {
        self.store = store
        self.playback = playback
    }

    func playEpisode(
        episodeID: EpisodeID,
        startSeconds: Double?,
        endSeconds: Double?,
        queuePosition: QueuePosition
    ) async -> PlayEpisodeResult? {
        await MainActor.run {
            guard let store, let playback,
                  let uuid = UUID(uuidString: episodeID),
                  let episode = store.episode(id: uuid) else {
                logger.error("playEpisode: unknown episode \(episodeID, privacy: .public)")
                return nil
            }
            let item = QueueItem(
                episodeID: uuid,
                startSeconds: startSeconds,
                endSeconds: endSeconds,
                label: nil
            )
            let podcastTitle = store.podcast(id: episode.podcastID)?.title
            switch queuePosition {
            case .now:
                // Replace current playback with this item; existing queue is
                // preserved and resumes after this finishes.
                playback.enqueueSegments([item], playNow: true) { store.episode(id: $0) }
                logger.info("playEpisode(now): \(episode.title, privacy: .public)")
                return PlayEpisodeResult(
                    episodeID: episodeID,
                    queuePosition: .now,
                    startedPlaying: true,
                    episodeTitle: episode.title,
                    podcastTitle: podcastTitle,
                    durationSeconds: episode.duration.map { Int($0) }
                )
            case .next:
                playback.insertNext(item)
                logger.info("playEpisode(next): \(episode.title, privacy: .public)")
                return PlayEpisodeResult(
                    episodeID: episodeID,
                    queuePosition: .next,
                    startedPlaying: false,
                    episodeTitle: episode.title,
                    podcastTitle: podcastTitle,
                    durationSeconds: episode.duration.map { Int($0) }
                )
            case .end:
                playback.enqueueItem(item)
                logger.info("playEpisode(end): \(episode.title, privacy: .public)")
                return PlayEpisodeResult(
                    episodeID: episodeID,
                    queuePosition: .end,
                    startedPlaying: false,
                    episodeTitle: episode.title,
                    podcastTitle: podcastTitle,
                    durationSeconds: episode.duration.map { Int($0) }
                )
            }
        }
    }

    func pausePlayback() async -> Bool {
        await MainActor.run {
            guard let playback else {
                logger.error("pausePlayback: playback host missing")
                return false
            }
            playback.pause()
            logger.info("pausePlayback: paused")
            return true
        }
    }

    func setPlaybackRate(_ rate: Double) async -> Double? {
        await MainActor.run {
            guard let playback else {
                logger.error("setPlaybackRate: playback host missing")
                return nil
            }
            let clamped = min(max(rate, 0.5), 3.0)
            playback.engine.setRate(clamped)
            logger.info("setPlaybackRate: \(clamped)")
            return clamped
        }
    }

    func setSleepTimer(mode: String, minutes: Int?) async -> String? {
        await MainActor.run {
            guard let playback else {
                logger.error("setSleepTimer: playback host missing")
                return nil
            }
            let timer: PlaybackSleepTimer
            switch mode {
            case "off":
                timer = .off
            case "end_of_episode":
                timer = .endOfEpisode
            case "minutes":
                timer = .minutes(max(1, minutes ?? 30))
            default:
                timer = .off
            }
            playback.setSleepTimer(timer)
            logger.info("setSleepTimer: \(timer.label, privacy: .public)")
            return timer.label
        }
    }

    func playExternalEpisode(
        audioURL: URL,
        title: String,
        feedURLString: String?,
        durationSeconds: TimeInterval?,
        startSeconds: Double?,
        endSeconds: Double?,
        queuePosition: QueuePosition
    ) async -> PlayEpisodeResult? {
        // Resolve which podcast to attach this episode to WITHOUT blocking
        // playback on a network fetch. Three cases:
        //   1. We already know about this feed (existing Podcast row) → use it.
        //   2. We don't know about it yet and a feed_url was supplied → use a
        //      thin placeholder Podcast(feedURL: …) now, then enrich its
        //      metadata in the background. The episode lives under that
        //      placeholder ID across the enrichment hop so its parent is
        //      stable for the user.
        //   3. No feed_url at all → parent to Podcast.unknownID.
        //
        // We deliberately never call `ensurePodcast` here: that helper also
        // upserts every parsed episode in the feed, which would dump the
        // show's whole backlog into the user's library without them having
        // subscribed. Backlog ingestion is reserved for `subscribe_podcast`.
        let parentResolution = await resolveExternalParent(feedURLString: feedURLString)
        guard let parentResolution else {
            logger.error("playExternalEpisode: store unavailable")
            return nil
        }
        let result: PlayEpisodeResult? = await MainActor.run {
            guard let store, let playback else {
                logger.error("playExternalEpisode: playback host missing")
                return nil
            }
            let episode = store.upsertEpisode(
                podcastID: parentResolution.podcastID,
                audioURL: audioURL,
                title: title,
                imageURL: nil,
                duration: durationSeconds
            )
            let podcastTitle = store.podcast(id: parentResolution.podcastID)?.title
            let item = QueueItem(
                episodeID: episode.id,
                startSeconds: startSeconds,
                endSeconds: endSeconds,
                label: nil
            )
            switch queuePosition {
            case .now:
                playback.enqueueSegments([item], playNow: true) { store.episode(id: $0) }
                logger.info("playExternalEpisode(now): '\(title, privacy: .public)' at \(startSeconds ?? 0)")
                return PlayEpisodeResult(
                    episodeID: episode.id.uuidString,
                    queuePosition: .now,
                    startedPlaying: true,
                    episodeTitle: episode.title,
                    podcastTitle: podcastTitle,
                    durationSeconds: episode.duration.map { Int($0) }
                )
            case .next:
                playback.insertNext(item)
                logger.info("playExternalEpisode(next): '\(title, privacy: .public)'")
                return PlayEpisodeResult(
                    episodeID: episode.id.uuidString,
                    queuePosition: .next,
                    startedPlaying: false,
                    episodeTitle: episode.title,
                    podcastTitle: podcastTitle,
                    durationSeconds: episode.duration.map { Int($0) }
                )
            case .end:
                playback.enqueueItem(item)
                logger.info("playExternalEpisode(end): '\(title, privacy: .public)'")
                return PlayEpisodeResult(
                    episodeID: episode.id.uuidString,
                    queuePosition: .end,
                    startedPlaying: false,
                    episodeTitle: episode.title,
                    podcastTitle: podcastTitle,
                    durationSeconds: episode.duration.map { Int($0) }
                )
            }
        }
        // Asynchronously hydrate podcast metadata in the background so the
        // first render shows whatever we have, and later renders pick up
        // real title / artwork once the feed comes back. Fire-and-forget;
        // playback doesn't depend on the result.
        if let feedURLString,
           parentResolution.shouldHydrateMetadata,
           let url = URL(string: feedURLString) {
            Task.detached { [weak self] in
                await self?.hydratePlaceholderPodcastMetadata(podcastID: parentResolution.podcastID, feedURL: url)
            }
        }
        return result
    }

    /// Decision wrapper: which podcast ID to parent the episode to RIGHT
    /// NOW, plus whether the caller should kick off a background metadata
    /// fetch to enrich a freshly-created placeholder.
    private struct ExternalParentResolution {
        let podcastID: UUID
        let shouldHydrateMetadata: Bool
    }

    /// Resolves (or creates a placeholder for) the parent podcast without
    /// hitting the network. The optional feed URL is normalized
    /// case-insensitively to match `store.podcast(feedURL:)`.
    @MainActor
    private func resolveExternalParent(feedURLString: String?) async -> ExternalParentResolution? {
        guard let store else { return nil }
        guard let feedURLString,
              let feedURL = URL(string: feedURLString),
              let scheme = feedURL.scheme?.lowercased(),
              scheme == "http" || scheme == "https" else {
            return ExternalParentResolution(podcastID: Podcast.unknownID, shouldHydrateMetadata: false)
        }
        if let existing = store.podcast(feedURL: feedURL) {
            return ExternalParentResolution(podcastID: existing.id, shouldHydrateMetadata: false)
        }
        // Insert a thin placeholder so the episode has a real parent. Title
        // defaults to the feed host so the UI shows something sensible
        // immediately; metadata hydration overwrites it on success.
        // titleIsPlaceholder stays true until hydration succeeds, letting
        // the UI render it as provisional.
        let placeholder = Podcast(
            kind: .rss,
            feedURL: feedURL,
            title: feedURL.host ?? feedURLString,
            titleIsPlaceholder: true
        )
        let stored = store.upsertPodcast(placeholder)
        return ExternalParentResolution(podcastID: stored.id, shouldHydrateMetadata: true)
    }

    /// Fetches the feed in the background and updates the placeholder
    /// `Podcast` row's title / author / artwork. Does NOT upsert episodes:
    /// the user hasn't followed this show, so we keep the library
    /// untouched (the user's external-played episode already exists).
    private func hydratePlaceholderPodcastMetadata(podcastID: UUID, feedURL: URL) async {
        guard let store else { return }
        let client = FeedClient()
        // Seed the fetch with a placeholder that carries the known podcast ID
        // so FeedClient preserves foreign keys. titleIsPlaceholder stays true
        // here; we clear it only when the fetch returns real metadata below.
        let placeholder = Podcast(
            id: podcastID,
            kind: .rss,
            feedURL: feedURL,
            title: feedURL.host ?? feedURL.absoluteString,
            titleIsPlaceholder: true
        )
        do {
            let result = try await client.fetch(placeholder)
            if case .updated(var podcast, _, _) = result {
                // Explicit clear — don't rely on FeedClient zero-initialising
                // a field it doesn't know about.
                podcast.titleIsPlaceholder = false
                await MainActor.run {
                    store.updatePodcast(podcast)
                }
            }
            // .notModified on a first hydration fetch means the server sent no
            // real title; leave titleIsPlaceholder = true in the store so the
            // next refresh can retry.
        } catch {
            logger.error(
                "playExternalEpisode: background metadata fetch failed for \(feedURL.absoluteString, privacy: .public): \(error.localizedDescription, privacy: .public)"
            )
            // titleIsPlaceholder remains true in the store; the next refresh
            // attempt will retry hydration.
        }
    }

}

// MARK: - Library adapter

final class LivePodcastLibraryAdapter: PodcastLibraryProtocol, @unchecked Sendable {

    weak var store: AppStateStore?
    private let downloadService: EpisodeDownloadService
    private let transcriptService: TranscriptIngestService
    private let refreshService: SubscriptionRefreshService

    init(
        store: AppStateStore,
        downloadService: EpisodeDownloadService,
        transcriptService: TranscriptIngestService,
        refreshService: SubscriptionRefreshService
    ) {
        self.store = store
        self.downloadService = downloadService
        self.transcriptService = transcriptService
        self.refreshService = refreshService
    }

    func markEpisodePlayed(episodeID: EpisodeID) async throws -> EpisodeMutationResult {
        try await mutateEpisode(episodeID: episodeID, state: "played") { store, id in
            store.markEpisodePlayed(id)
        }
    }

    func markEpisodeUnplayed(episodeID: EpisodeID) async throws -> EpisodeMutationResult {
        try await mutateEpisode(episodeID: episodeID, state: "unplayed") { store, id in
            store.markEpisodeUnplayed(id)
        }
    }

    func downloadEpisode(episodeID: EpisodeID) async throws -> EpisodeMutationResult {
        try await mutateEpisode(episodeID: episodeID, state: nil) { store, id in
            self.downloadService.attach(appStore: store)
            self.downloadService.download(episodeID: id)
        }
    }

    func requestTranscription(episodeID: EpisodeID) async throws -> TranscriptRequestResult {
        guard let uuid = UUID(uuidString: episodeID) else {
            throw PodcastAgentToolAdapterError.invalidID(episodeID)
        }
        guard let store else {
            throw PodcastAgentToolAdapterError.unavailable("AppStateStore")
        }
        guard let episode = await store.episode(id: uuid) else {
            throw PodcastAgentToolAdapterError.missingEpisode(episodeID)
        }
        if case .ready(let source) = episode.transcriptState {
            return TranscriptRequestResult(
                episodeID: episodeID,
                status: "ready",
                source: source.rawValue
            )
        }
        await MainActor.run {
            store.setEpisodeTranscriptState(uuid, state: .queued)
            Task { @MainActor in
                await self.transcriptService.ingest(episodeID: uuid)
            }
        }
        return TranscriptRequestResult(
            episodeID: episodeID,
            status: "queued",
            message: "Transcript ingestion started."
        )
    }

    func downloadAndTranscribe(episodeID: EpisodeID) async throws -> TranscriptRequestResult {
        guard let uuid = UUID(uuidString: episodeID) else {
            throw PodcastAgentToolAdapterError.invalidID(episodeID)
        }
        guard let store else {
            throw PodcastAgentToolAdapterError.unavailable("AppStateStore")
        }
        guard let episode = await store.episode(id: uuid) else {
            throw PodcastAgentToolAdapterError.missingEpisode(episodeID)
        }
        // Already transcribed — skip the pipeline.
        if case .ready(let source) = episode.transcriptState {
            return TranscriptRequestResult(
                episodeID: episodeID,
                status: "ready",
                source: source.rawValue,
                message: "Transcript already available."
            )
        }
        // Kick off download for offline playback (fire-and-forget).
        // Transcription below will use the remote enclosure URL while
        // the download proceeds in the background; Apple-native STT
        // will pick up the local file on the post-download ingest trigger.
        await MainActor.run {
            downloadService.attach(appStore: store)
            downloadService.download(episodeID: uuid)
        }
        // Await the full transcription pipeline. This call blocks until the
        // transcript is persisted (.ready) or the pipeline gives up (.failed).
        await transcriptService.ingest(episodeID: uuid)
        // Read the final state.
        let final = await store.episode(id: uuid) ?? episode
        switch final.transcriptState {
        case .ready(let src):
            return TranscriptRequestResult(
                episodeID: episodeID,
                status: "ready",
                source: src.rawValue
            )
        case .failed(let msg):
            return TranscriptRequestResult(
                episodeID: episodeID,
                status: "failed",
                message: msg
            )
        default:
            return TranscriptRequestResult(
                episodeID: episodeID,
                status: "unavailable",
                message: "Transcription could not complete. Check STT provider settings."
            )
        }
    }

    func createClip(
        episodeID: EpisodeID,
        startSeconds: Double,
        endSeconds: Double,
        caption: String?,
        transcriptText: String?
    ) async throws -> ClipResult {
        guard let uuid = UUID(uuidString: episodeID) else {
            throw PodcastAgentToolAdapterError.invalidID(episodeID)
        }
        guard let store else {
            throw PodcastAgentToolAdapterError.unavailable("AppStateStore")
        }
        guard let episode = await store.episode(id: uuid) else {
            throw PodcastAgentToolAdapterError.missingEpisode(episodeID)
        }
        let startMs = Int(startSeconds * 1000)
        let endMs = Int(endSeconds * 1000)
        let resolvedText: String
        if let supplied = transcriptText, !supplied.isEmpty {
            resolvedText = supplied
        } else {
            resolvedText = Self.extractTranscriptText(
                episodeID: uuid,
                startSeconds: startSeconds,
                endSeconds: endSeconds
            )
        }
        let clip = await MainActor.run {
            store.addClip(
                episodeID: uuid,
                subscriptionID: episode.podcastID,
                startMs: startMs,
                endMs: endMs,
                transcriptText: resolvedText,
                source: .agent,
                caption: caption
            )
        }
        return ClipResult(
            clipID: clip.id.uuidString,
            episodeID: episodeID,
            podcastID: episode.podcastID.uuidString,
            episodeTitle: episode.title,
            startSeconds: startSeconds,
            endSeconds: endSeconds,
            transcriptText: resolvedText,
            caption: caption
        )
    }

    private static func extractTranscriptText(
        episodeID: UUID,
        startSeconds: Double,
        endSeconds: Double
    ) -> String {
        guard let transcript = TranscriptStore.shared.load(episodeID: episodeID) else { return "" }
        let matching = transcript.segments.filter { $0.end > startSeconds && $0.start < endSeconds }
        return matching.map(\.text).joined(separator: " ")
    }

    func refreshFeed(podcastID: PodcastID) async throws -> FeedRefreshResult {
        guard let uuid = UUID(uuidString: podcastID) else {
            throw PodcastAgentToolAdapterError.invalidID(podcastID)
        }
        guard let store else {
            throw PodcastAgentToolAdapterError.unavailable("AppStateStore")
        }
        guard let before = await store.podcast(id: uuid) else {
            throw PodcastAgentToolAdapterError.missingPodcast(podcastID)
        }
        let priorCount = await store.episodes(forPodcast: uuid).count
        try await refreshService.refresh(uuid, store: store)
        let after = await store.podcast(id: uuid) ?? before
        let episodeCount = await store.episodes(forPodcast: uuid).count
        return FeedRefreshResult(
            podcastID: podcastID,
            title: after.title,
            episodeCount: episodeCount,
            newEpisodeCount: max(0, episodeCount - priorCount),
            refreshedAt: after.lastRefreshedAt
        )
    }

    private func mutateEpisode(
        episodeID: EpisodeID,
        state explicitState: String?,
        _ mutation: @escaping @MainActor (AppStateStore, UUID) -> Void
    ) async throws -> EpisodeMutationResult {
        guard let uuid = UUID(uuidString: episodeID) else {
            throw PodcastAgentToolAdapterError.invalidID(episodeID)
        }
        guard let store else {
            throw PodcastAgentToolAdapterError.unavailable("AppStateStore")
        }
        guard let before = await store.episode(id: uuid) else {
            throw PodcastAgentToolAdapterError.missingEpisode(episodeID)
        }
        await MainActor.run {
            mutation(store, uuid)
        }
        let after = await store.episode(id: uuid) ?? before
        let subscription = await store.podcast(id: after.podcastID)
        return EpisodeMutationResult(
            episodeID: episodeID,
            podcastID: after.podcastID.uuidString,
            episodeTitle: after.title,
            podcastTitle: subscription?.title,
            state: explicitState ?? Self.downloadStateLabel(after.downloadState)
        )
    }

    private static func downloadStateLabel(_ state: DownloadState) -> String {
        switch state {
        case .notDownloaded: return "not_downloaded"
        case .queued: return "queued"
        case .downloading: return "downloading"
        case .downloaded: return "downloaded"
        case .failed: return "failed"
        }
    }
}

enum PodcastAgentToolAdapterError: LocalizedError {
    case unavailable(String)
    case invalidID(String)
    case missingEpisode(String)
    case missingPodcast(String)

    var errorDescription: String? {
        switch self {
        case .unavailable(let name): return "\(name) is unavailable."
        case .invalidID(let value): return "Invalid UUID: \(value)"
        case .missingEpisode(let id): return "Episode not found: \(id)"
        case .missingPodcast(let id): return "Podcast not found: \(id)"
        }
    }
}
