import Foundation
import os.log

// MARK: - LiveYouTubeIngestionAdapter
//
// Live implementation of `YouTubeIngestionProtocol`. Steps:
//   1. Look up the configured extractor URL from `AppStateStore` settings.
//   2. Call `YouTubeAudioService` to resolve audio stream URL + metadata.
//   3. Download the audio file to a temp location, then move to the
//      agent-episodes directory (same layout as TTS-generated episodes).
//   4. Publish the episode to the "Agent Generated" podcast via
//      `AgentGeneratedPodcastService.publishEpisode`.
//   5. Optionally enqueue transcription via `TranscriptIngestService`.

final class LiveYouTubeIngestionAdapter: YouTubeIngestionProtocol, @unchecked Sendable {

    private static let logger = Logger.app("LiveYouTubeIngestionAdapter")

    private let ytService = YouTubeAudioService()
    weak var store: AppStateStore?

    init(store: AppStateStore) {
        self.store = store
    }

    func ingestVideo(
        youtubeURL: String,
        customTitle: String?,
        transcribe: Bool
    ) async throws -> YouTubeIngestionResult {
        guard let store else {
            throw YouTubeAudioServiceError.notConfigured
        }
        let extractorURL = await store.state.settings.youtubeExtractorURL ?? ""
        guard !extractorURL.isEmpty else {
            throw YouTubeAudioServiceError.notConfigured
        }

        // Step 1 — resolve audio stream URL + metadata
        Self.logger.info("Fetching YouTube video info for \(youtubeURL, privacy: .public)")
        let info = try await ytService.fetchVideoInfo(
            youtubeURL: youtubeURL,
            extractorURLString: extractorURL
        )

        let finalTitle = customTitle.flatMap { $0.trimmingCharacters(in: .whitespacesAndNewlines).nilIfEmpty } ?? info.title

        // Step 2 — download audio to a temp file
        let episodeID = UUID()
        let destURL = try AgentGeneratedPodcastService.audioFileURL(episodeID: episodeID)
        Self.logger.info("Downloading audio to \(destURL.lastPathComponent, privacy: .public)")
        try await downloadAudio(from: info.audioURL, to: destURL)

        // Step 3 — publish episode to "Agent Generated" podcast
        let episode = await MainActor.run {
            AgentGeneratedPodcastService.publishEpisode(
                title: finalTitle,
                description: "From YouTube: \(youtubeURL)",
                audioURL: destURL,
                durationSeconds: info.durationSeconds,
                in: store
            )
        }
        Self.logger.info("Published YouTube episode \(episode.id, privacy: .public) '\(finalTitle, privacy: .public)'")

        // Step 4 — optional transcription
        var transcriptStatus: String?
        if transcribe {
            Self.logger.info("Enqueuing transcription for \(episode.id, privacy: .public)")
            await TranscriptIngestService.shared.ingest(episodeID: episode.id)
            let refreshed = await store.episode(id: episode.id)
            transcriptStatus = switch refreshed?.transcriptState {
            case .ready: "ready"
            case .failed: "failed"
            default: "queued"
            }
        }

        return YouTubeIngestionResult(
            episodeID: episode.id.uuidString,
            title: finalTitle,
            author: info.author,
            durationSeconds: info.durationSeconds,
            transcriptStatus: transcriptStatus
        )
    }

    // MARK: - Search

    func searchVideos(query: String, limit: Int) async throws -> [YouTubeSearchResult] {
        guard let store else { throw YouTubeAudioServiceError.notConfigured }
        let extractorURL = await store.state.settings.youtubeExtractorURL ?? ""
        guard !extractorURL.isEmpty else { throw YouTubeAudioServiceError.notConfigured }
        return try await ytService.searchVideos(query: query, limit: limit, extractorURLString: extractorURL)
    }

    // MARK: - Download helper

    private func downloadAudio(from remoteURL: URL, to destURL: URL) async throws {
        let (tempURL, response) = try await URLSession.shared.download(from: remoteURL)
        if let http = response as? HTTPURLResponse, !(200..<300).contains(http.statusCode) {
            throw YouTubeAudioServiceError.requestFailed(http.statusCode, "Audio download failed")
        }
        try FileManager.default.moveItem(at: tempURL, to: destURL)
    }
}

