import Foundation

// MARK: - Library adapter

final class LivePodcastLibraryAdapter: PodcastLibraryProtocol, @unchecked Sendable {

    weak var store: AppStateStore?
    private let transcriptService: TranscriptIngestService

    init(
        store: AppStateStore,
        transcriptService: TranscriptIngestService
    ) {
        self.store = store
        self.transcriptService = transcriptService
    }

    func markEpisodePlayed(episodeID: EpisodeID) async throws -> EpisodeMutationResult {
        try await mutateEpisodeViaKernel(episodeID: episodeID, state: "played") { store, id in
            store.kernelMarkPlayed(episodeID: id)
        }
    }

    func markEpisodeUnplayed(episodeID: EpisodeID) async throws -> EpisodeMutationResult {
        try await mutateEpisodeViaKernel(episodeID: episodeID, state: "unplayed") { store, id in
            store.kernelMarkUnplayed(episodeID: id)
        }
    }

    func downloadEpisode(episodeID: EpisodeID) async throws -> EpisodeMutationResult {
        try await mutateEpisodeViaKernel(episodeID: episodeID, state: "queued") { store, id in
            store.kernelDownload(episodeID: id)
        }
    }

    func requestTranscription(episodeID: EpisodeID) async throws -> TranscriptRequestResult {
        guard let store else {
            throw PodcastAgentToolAdapterError.unavailable("AppStateStore")
        }
        guard let uuid = UUID(uuidString: episodeID) else {
            throw PodcastAgentToolAdapterError.invalidID(episodeID)
        }
        let existing = try await transcriptToolResult(store: store, episodeID: episodeID, uuid: uuid)
        if existing.status == "ready" {
            return existing
        }

        let result = await MainActor.run {
            store.kernelReportEpisodeTranscriptState(episodeID: uuid, state: .queued)
        }
        guard let result else {
            throw PodcastAgentToolAdapterError.unavailable("Rust kernel")
        }
        if case let .failure(message) = result {
            throw PodcastAgentToolAdapterError.rejected(message)
        }
        await MainActor.run {
            Task { @MainActor in
                await self.transcriptService.ingest(episodeID: uuid)
            }
        }
        return try await transcriptToolResult(store: store, episodeID: episodeID, uuid: uuid)
    }

    func downloadAndTranscribe(episodeID: EpisodeID) async throws -> TranscriptRequestResult {
        guard let store else {
            throw PodcastAgentToolAdapterError.unavailable("AppStateStore")
        }
        guard let uuid = UUID(uuidString: episodeID) else {
            throw PodcastAgentToolAdapterError.invalidID(episodeID)
        }
        let existing = try await transcriptToolResult(store: store, episodeID: episodeID, uuid: uuid)
        if existing.status == "ready" {
            return existing
        }
        let statusResult = await MainActor.run {
            store.kernelReportEpisodeTranscriptState(episodeID: uuid, state: .queued)
        }
        guard let statusResult else {
            throw PodcastAgentToolAdapterError.unavailable("Rust kernel")
        }
        if case let .failure(message) = statusResult {
            throw PodcastAgentToolAdapterError.rejected(message)
        }
        // Kick off download for offline playback (fire-and-forget).
        // Transcription below will use the remote enclosure URL while
        // the download proceeds in the background; Apple-native STT
        // will pick up the local file on the post-download ingest trigger.
        let downloadResult = await MainActor.run {
            store.kernelDownload(episodeID: episodeID)
        }
        if case let .some(.failure(message)) = downloadResult {
            throw PodcastAgentToolAdapterError.rejected(message)
        }
        // Execute the native capability branch, then ask Rust how to report the
        // resulting transcript state to the agent.
        await transcriptService.ingest(episodeID: uuid)
        return try await transcriptToolResult(store: store, episodeID: episodeID, uuid: uuid)
    }

    func createClip(
        episodeID: EpisodeID,
        startSeconds: Double,
        endSeconds: Double,
        caption: String?,
        transcriptText _: String?
    ) async throws -> ClipResult {
        guard let store else {
            throw PodcastAgentToolAdapterError.unavailable("AppStateStore")
        }
        let clipID = UUID()
        let result = await MainActor.run {
            store.kernelCreateClip(
                id: clipID,
                episodeID: episodeID,
                startSecs: startSeconds,
                endSecs: endSeconds,
                title: caption,
                source: .agent
            )
        }
        guard let result else {
            throw PodcastAgentToolAdapterError.unavailable("Rust kernel")
        }
        if case let .failure(message) = result {
            throw PodcastAgentToolAdapterError.rejected(message)
        }
        guard let uuid = UUID(uuidString: episodeID) else {
            throw PodcastAgentToolAdapterError.invalidID(episodeID)
        }
        guard let episode = await store.episode(id: uuid) else {
            throw PodcastAgentToolAdapterError.missingEpisode(episodeID)
        }
        let createdClip = await store.clip(id: clipID)
        let normalizedStart = createdClip.map { Double($0.startMs) / 1000 } ?? min(startSeconds, endSeconds)
        let normalizedEnd = createdClip.map { Double($0.endMs) / 1000 } ?? max(startSeconds, endSeconds)
        return ClipResult(
            clipID: clipID.uuidString,
            episodeID: episodeID,
            podcastID: episode.podcastID.uuidString,
            episodeTitle: episode.title,
            startSeconds: normalizedStart,
            endSeconds: normalizedEnd,
            transcriptText: createdClip?.transcriptText ?? "",
            caption: caption
        )
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
        let priorCount = await MainActor.run { store.rustEpisodeCount(forPodcast: uuid) }
        await MainActor.run { store.kernelRefresh(podcastID: uuid) }
        let after = await store.podcast(id: uuid) ?? before
        let episodeCount = await MainActor.run { store.rustEpisodeCount(forPodcast: uuid) }
        return FeedRefreshResult(
            podcastID: podcastID,
            title: after.title,
            episodeCount: episodeCount,
            newEpisodeCount: max(0, episodeCount - priorCount),
            refreshedAt: after.lastRefreshedAt
        )
    }

    private func mutateEpisodeViaKernel(
        episodeID: EpisodeID,
        state explicitState: String,
        _ mutation: @escaping @MainActor (AppStateStore, String) -> DispatchResult?
    ) async throws -> EpisodeMutationResult {
        guard let store else {
            throw PodcastAgentToolAdapterError.unavailable("AppStateStore")
        }
        let result = await MainActor.run {
            mutation(store, episodeID)
        }
        guard let result else {
            throw PodcastAgentToolAdapterError.unavailable("Rust kernel")
        }
        if case let .failure(message) = result {
            throw PodcastAgentToolAdapterError.rejected(message)
        }
        let toolResult = await MainActor.run {
            store.kernel?.episodeMutationToolResult(episodeID: episodeID, state: explicitState)
        }
        guard let toolResult else {
            throw PodcastAgentToolAdapterError.unavailable("Rust kernel")
        }
        guard toolResult.ok else {
            throw PodcastAgentToolAdapterError.rejected(
                toolResult.message ?? "Episode mutation result was rejected by the kernel."
            )
        }
        return EpisodeMutationResult(
            episodeID: toolResult.episodeId,
            podcastID: toolResult.podcastId,
            episodeTitle: toolResult.episodeTitle,
            podcastTitle: toolResult.podcastTitle,
            state: toolResult.state
        )
    }

    private func transcriptToolResult(
        store: AppStateStore,
        episodeID: EpisodeID,
        uuid: UUID
    ) async throws -> TranscriptRequestResult {
        let result = await MainActor.run {
            store.kernel?.transcriptToolResult(episodeID: uuid)
        }
        guard let result else {
            throw PodcastAgentToolAdapterError.unavailable("Rust kernel")
        }
        guard result.ok else {
            throw PodcastAgentToolAdapterError.rejected(
                result.message ?? "Transcript status was rejected by the kernel."
            )
        }
        return TranscriptRequestResult(
            episodeID: episodeID,
            status: result.status,
            source: result.source,
            message: result.message
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
