import Foundation

// MARK: - Search models

struct PodcastLocalSearchResults: Sendable {
    var shows: [PodcastShowSearchHit] = []
    var episodes: [PodcastEpisodeSearchHit] = []

    var isEmpty: Bool {
        shows.isEmpty && episodes.isEmpty
    }
}

struct PodcastShowSearchHit: Identifiable, Hashable, Sendable {
    var podcast: Podcast
    var score: Int
    var id: UUID { podcast.id }
}

struct PodcastEpisodeSearchHit: Identifiable, Hashable, Sendable {
    var episode: Episode
    var podcast: Podcast
    var snippet: String
    var score: Int
    var id: UUID { episode.id }
}


enum PodcastSearchEngine {
    static func localResults(
        query: String,
        store: AppStateStore,
        limit: Int = 8
    ) -> PodcastLocalSearchResults {
        let trimmed = query.trimmed
        guard !trimmed.isEmpty else { return PodcastLocalSearchResults() }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        guard let envelope = store.kernel?.localSearchEnvelope(query: trimmed, limit: limit),
              let data = envelope.data(using: .utf8),
              let decoded = try? decoder.decode(KernelLocalSearchEnvelope.self, from: data)
        else { return PodcastLocalSearchResults() }
        return PodcastLocalSearchResults(
            shows: decoded.shows.compactMap { row in
                guard let id = UUID(uuidString: row.podcastId),
                      let podcast = store.podcast(id: id) else { return nil }
                return PodcastShowSearchHit(podcast: podcast, score: row.score)
            },
            episodes: decoded.episodes.compactMap { row in
                guard let episodeID = UUID(uuidString: row.episodeId),
                      let podcastID = UUID(uuidString: row.podcastId),
                      let episode = store.episode(id: episodeID),
                      let podcast = store.podcast(id: podcastID) else { return nil }
                return PodcastEpisodeSearchHit(
                    episode: episode,
                    podcast: podcast,
                    snippet: row.snippet,
                    score: row.score
                )
            }
        )
    }
}

private struct KernelLocalSearchEnvelope: Decodable {
    var shows: [KernelShowHit] = []
    var episodes: [KernelEpisodeHit] = []
}

private struct KernelShowHit: Decodable {
    var podcastId: String
    var score: Int
}

private struct KernelEpisodeHit: Decodable {
    var episodeId: String
    var podcastId: String
    var snippet: String
    var score: Int
}
