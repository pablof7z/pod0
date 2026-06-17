import Foundation

// MARK: - LibraryPodcastStatsProjection
//
// Thin decoder for Rust-owned podcast library facts. Swift passes the visible
// podcast ids and renders labels; Rust owns the episode counts.

struct LibraryPodcastStatsProjection {
    let episodeCounts: [UUID: Int]
    let unplayedCounts: [UUID: Int]
    let downloadedPodcastIDs: Set<UUID>
    let transcribedPodcastIDs: Set<UUID>
    let latestEpisodeIDs: [UUID: UUID]

    static func load(podcastIDs: [UUID], store: AppStateStore) -> LibraryPodcastStatsProjection {
        guard !podcastIDs.isEmpty,
              let envelope = store.kernel?.libraryPodcastStatsEnvelope(podcastIDs: podcastIDs),
              let data = envelope.data(using: .utf8),
              let decoded = try? JSONDecoder.libraryPodcastStats.decode(Response.self, from: data)
        else {
            return LibraryPodcastStatsProjection(
                episodeCounts: [:],
                unplayedCounts: [:],
                downloadedPodcastIDs: [],
                transcribedPodcastIDs: [],
                latestEpisodeIDs: [:]
            )
        }

        return LibraryPodcastStatsProjection(
            episodeCounts: Dictionary(
                uniqueKeysWithValues: decoded.podcasts.map { ($0.podcastId, $0.episodeCount) }
            ),
            unplayedCounts: Dictionary(
                uniqueKeysWithValues: decoded.podcasts.map { ($0.podcastId, $0.unplayedCount) }
            ),
            downloadedPodcastIDs: Set(
                decoded.podcasts.compactMap { $0.hasDownloadedEpisode ? $0.podcastId : nil }
            ),
            transcribedPodcastIDs: Set(
                decoded.podcasts.compactMap { $0.hasTranscribedEpisode ? $0.podcastId : nil }
            ),
            latestEpisodeIDs: Dictionary(
                uniqueKeysWithValues: decoded.podcasts.compactMap { row in
                    row.latestEpisodeId.map { (row.podcastId, $0) }
                }
            )
        )
    }

    func episodeCount(for podcastID: UUID) -> Int {
        episodeCounts[podcastID] ?? 0
    }

    func unplayedCount(for podcastID: UUID) -> Int {
        unplayedCounts[podcastID] ?? 0
    }

    func hasDownloadedEpisode(for podcastID: UUID) -> Bool {
        downloadedPodcastIDs.contains(podcastID)
    }

    func hasTranscribedEpisode(for podcastID: UUID) -> Bool {
        transcribedPodcastIDs.contains(podcastID)
    }

    func latestEpisode(for podcastID: UUID, store: AppStateStore) -> Episode? {
        guard let episodeID = latestEpisodeIDs[podcastID] else { return nil }
        return store.episode(id: episodeID)
    }

    private struct Response: Decodable {
        let podcasts: [Row]
    }

    private struct Row: Decodable {
        let podcastId: UUID
        let episodeCount: Int
        let unplayedCount: Int
        let hasDownloadedEpisode: Bool
        let hasTranscribedEpisode: Bool
        let latestEpisodeId: UUID?
    }
}

private extension JSONDecoder {
    static let libraryPodcastStats: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return decoder
    }()
}
