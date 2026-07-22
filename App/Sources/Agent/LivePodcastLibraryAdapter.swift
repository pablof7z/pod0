import Foundation

final class LivePodcastLibraryAdapter: PodcastLibraryProtocol, @unchecked Sendable {
    weak var store: AppStateStore?
    private let refreshService: SubscriptionRefreshService
    private let transcriptReader: any TranscriptReading

    init(
        store: AppStateStore,
        refreshService: SubscriptionRefreshService,
        transcriptReader: any TranscriptReading
    ) {
        self.store = store
        self.refreshService = refreshService
        self.transcriptReader = transcriptReader
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
            store.sharedLibrary?.requestDownload(episodeID: id)
        }
    }

    func requestTranscription(episodeID: EpisodeID) async throws -> TranscriptRequestResult {
        guard let uuid = UUID(uuidString: episodeID) else {
            throw PodcastAgentToolAdapterError.invalidID(episodeID)
        }
        guard let store else { throw PodcastAgentToolAdapterError.unavailable("AppStateStore") }
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
            WorkflowRuntime.shared.requestTranscript(episodeID: uuid)
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
        guard let store else { throw PodcastAgentToolAdapterError.unavailable("AppStateStore") }
        guard let episode = await store.episode(id: uuid) else {
            throw PodcastAgentToolAdapterError.missingEpisode(episodeID)
        }
        if case .ready(let source) = episode.transcriptState {
            return TranscriptRequestResult(
                episodeID: episodeID,
                status: "ready",
                source: source.rawValue,
                message: "Transcript already available."
            )
        }
        await MainActor.run {
            store.sharedLibrary?.requestDownload(episodeID: uuid)
            WorkflowRuntime.shared.requestTranscript(episodeID: uuid)
        }
        return TranscriptRequestResult(
            episodeID: episodeID,
            status: "queued",
            message: "Download and durable transcript processing were queued."
        )
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
        guard let store else { throw PodcastAgentToolAdapterError.unavailable("AppStateStore") }
        guard let episode = await store.episode(id: uuid) else {
            throw PodcastAgentToolAdapterError.missingEpisode(episodeID)
        }
        guard startSeconds.isFinite,
              endSeconds.isFinite,
              let startMs = Int(exactly: (startSeconds * 1_000).rounded(.towardZero)),
              let endMs = Int(exactly: (endSeconds * 1_000).rounded(.towardZero)),
              startMs >= 0,
              endMs > startMs
        else { throw PodcastAgentToolAdapterError.invalidClipBounds }
        let resolvedText: String
        if let supplied = transcriptText, !supplied.isEmpty {
            resolvedText = supplied
        } else {
            resolvedText = extractTranscriptText(
                episodeID: uuid,
                startSeconds: startSeconds,
                endSeconds: endSeconds
            )
        }
        guard let clip = await MainActor.run(body: {
            store.addClip(
                episodeID: uuid,
                subscriptionID: episode.podcastID,
                startMs: startMs,
                endMs: endMs,
                transcriptText: resolvedText,
                source: .agent,
                caption: caption
            )
        }) else {
            throw PodcastAgentToolAdapterError.unavailable("Shared clip core")
        }
        return ClipResult(
            clipID: clip.id.uuidString,
            episodeID: episodeID,
            podcastID: episode.podcastID.uuidString,
            episodeTitle: episode.title,
            startSeconds: Double(clip.startMs) / 1_000,
            endSeconds: Double(clip.endMs) / 1_000,
            transcriptText: clip.transcriptText,
            caption: clip.caption
        )
    }

    func refreshFeed(podcastID: PodcastID) async throws -> FeedRefreshResult {
        guard let uuid = UUID(uuidString: podcastID) else {
            throw PodcastAgentToolAdapterError.invalidID(podcastID)
        }
        guard let store else { throw PodcastAgentToolAdapterError.unavailable("AppStateStore") }
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

    private func extractTranscriptText(
        episodeID: UUID,
        startSeconds: Double,
        endSeconds: Double
    ) -> String {
        guard let transcript = transcriptReader.load(episodeID: episodeID) else { return "" }
        let matching = transcript.segments.filter { $0.end > startSeconds && $0.start < endSeconds }
        return matching.map(\.text).joined(separator: " ")
    }

    private func mutateEpisode(
        episodeID: EpisodeID,
        state explicitState: String?,
        _ mutation: @escaping @MainActor (AppStateStore, UUID) -> Void
    ) async throws -> EpisodeMutationResult {
        guard let uuid = UUID(uuidString: episodeID) else {
            throw PodcastAgentToolAdapterError.invalidID(episodeID)
        }
        guard let store else { throw PodcastAgentToolAdapterError.unavailable("AppStateStore") }
        guard let before = await store.episode(id: uuid) else {
            throw PodcastAgentToolAdapterError.missingEpisode(episodeID)
        }
        await MainActor.run { mutation(store, uuid) }
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
        case .notDownloaded: "not_downloaded"
        case .downloaded: "downloaded"
        }
    }
}
