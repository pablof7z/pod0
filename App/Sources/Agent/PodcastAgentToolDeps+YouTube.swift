import Foundation

// MARK: - YouTubeIngestionProtocol
//
// Abstracts the full YouTube-video → library-episode pipeline so the tool
// handler in `AgentTools+YouTube.swift` stays testable without live services.
//
// The live adapter (`LiveYouTubeIngestionAdapter`) calls:
//   1. `YouTubeAudioService` to resolve the audio stream URL + metadata.
//   2. `URLSession` to download the audio to the agent-episodes directory.
//   3. `AgentGeneratedPodcastService.publishEpisode` to register the episode.
//   4. Optionally `TranscriptIngestService` to kick off transcription.

protocol YouTubeIngestionProtocol: Sendable {
    func ingestVideo(
        youtubeURL: String,
        customTitle: String?,
        transcribe: Bool
    ) async throws -> YouTubeIngestionResult

    func searchVideos(query: String, limit: Int) async throws -> [YouTubeSearchResult]
}

// MARK: - Result

struct YouTubeIngestionResult: Sendable {
    let episodeID: String
    let title: String
    let author: String
    let durationSeconds: TimeInterval?
    let transcriptStatus: String?

    init(
        episodeID: String,
        title: String,
        author: String,
        durationSeconds: TimeInterval?,
        transcriptStatus: String?
    ) {
        self.episodeID = episodeID
        self.title = title
        self.author = author
        self.durationSeconds = durationSeconds
        self.transcriptStatus = transcriptStatus
    }
}
