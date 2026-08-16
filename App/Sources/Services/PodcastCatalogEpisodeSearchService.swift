import Foundation

/// Thin projection adapter over the Rust-owned durable catalog workflow.
@MainActor
struct PodcastCatalogEpisodeSearchService {
    struct SearchResult: Sendable {
        let episodes: [Episode]
        let boundedResult: String
    }

    enum SearchError: Error {
        case noMatches
    }

    func search(
        episodeQuery: String,
        podcastHint: String?,
        limit: Int,
        store: AppStateStore
    ) async throws -> SearchResult {
        guard let sharedLibrary = store.sharedLibrary else {
            throw SharedLibraryError.unavailable
        }
        let result = try await sharedLibrary.searchPodcastCatalog(
            episodeQuery: episodeQuery,
            podcastHint: podcastHint,
            limit: UInt16(clamping: limit)
        )
        let episodes = result.episodeIDs.compactMap(store.episode(id:))
        guard !episodes.isEmpty else { throw SearchError.noMatches }
        return SearchResult(episodes: episodes, boundedResult: result.boundedResult)
    }
}
