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
    private struct YouTubeIngestMetadataPlan: Decodable {
        let title: String
        let description: String
    }

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

        let metadata = await youtubeIngestMetadata(
            youtubeURL: youtubeURL,
            customTitle: customTitle,
            fallbackTitle: info.title
        )
        guard let metadata else {
            throw YouTubeAudioServiceError.requestFailed(0, "YouTube ingest metadata policy is unavailable")
        }

        // Step 2 — download audio to a temp file
        let episodeID = UUID()
        let destURL = try AgentGeneratedPodcastService.audioFileURL(episodeID: episodeID)
        Self.logger.info("Downloading audio to \(destURL.lastPathComponent, privacy: .public)")
        try await downloadAudio(from: info.audioURL, to: destURL)

        // Step 3 — publish episode to "Agent Generated" podcast
        let episode = try await MainActor.run {
            try AgentGeneratedPodcastService.publishEpisode(
                title: metadata.title,
                description: metadata.description,
                audioURL: destURL,
                durationSeconds: info.durationSeconds,
                in: store
            )
        }
        Self.logger.info("Published YouTube episode \(episode.id, privacy: .public) '\(metadata.title, privacy: .public)'")

        // Step 4 — optional transcription
        var transcriptStatus: String?
        if transcribe {
            Self.logger.info("Enqueuing transcription for \(episode.id, privacy: .public)")
            await TranscriptIngestService.shared.ingest(episodeID: episode.id)
            let refreshed = await store.episode(id: episode.id)
            transcriptStatus = Self.transcriptResultStatus(for: refreshed?.transcriptState)
        }

        return YouTubeIngestionResult(
            episodeID: episode.id.uuidString,
            title: metadata.title,
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

    private struct TranscriptResultStatusResponse: Decodable {
        let status: String?
        let error: String?
    }

    private static func transcriptResultStatus(for state: TranscriptState?) -> String? {
        guard let handle = KernelModel.shared?.podcastHandlePointer else { return nil }
        var request = state.map(AppStateStore.transcriptStatePayload) ?? ["state": "missing"]
        request["op"] = "transcript_result_status"
        guard let data = try? JSONSerialization.data(withJSONObject: request),
              let json = String(data: data, encoding: .utf8)
        else { return nil }
        let envelope = json.withCString { ptr -> String? in
            guard let result = nmp_app_podcast_agent_action_tool(handle, ptr) else {
                return nil
            }
            defer { nmp_free_string(result) }
            return String(cString: result)
        }
        guard let envelope,
              let responseData = envelope.data(using: .utf8),
              let response = try? JSONDecoder().decode(TranscriptResultStatusResponse.self, from: responseData),
              response.error == nil
        else { return nil }
        return response.status
    }

    private func youtubeIngestMetadata(
        youtubeURL: String,
        customTitle: String?,
        fallbackTitle: String
    ) async -> YouTubeIngestMetadataPlan? {
        let handleBits = await MainActor.run {
            KernelModel.shared?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }
        var payload: [String: Any] = [
            "op": "youtube_ingest_metadata",
            "url": youtubeURL,
            "fallback_title": fallbackTitle,
        ]
        if let customTitle { payload["custom_title"] = customTitle }
        guard let handleBits,
              let data = try? JSONSerialization.data(withJSONObject: payload),
              let json = String(data: data, encoding: .utf8)
        else { return nil }
        return await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return nil
            }
            return json.withCString { ptr -> YouTubeIngestMetadataPlan? in
                guard let result = nmp_app_podcast_agent_action_tool(handle, ptr) else {
                    return nil
                }
                defer { nmp_free_string(result) }
                let envelope = String(cString: result)
                guard let data = envelope.data(using: .utf8) else { return nil }
                return try? JSONDecoder().decode(YouTubeIngestMetadataPlan.self, from: data)
            }
        }.value
    }
}
