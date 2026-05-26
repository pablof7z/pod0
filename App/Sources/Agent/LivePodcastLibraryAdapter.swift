import Foundation

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
