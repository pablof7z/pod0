import Foundation

// MARK: - AllEpisodesProjection
//
// Rust owns all-episode membership, filter/search predicates, newest-first
// ordering, archive visibility, and total filtered count. Swift resolves ids
// and renders native rows.

struct AllEpisodesProjection {
    let episodeIDs: [UUID]
    let totalCount: Int

    static func load(
        filter: AllEpisodesFilter,
        query: String,
        limit: Int,
        store: AppStateStore
    ) -> AllEpisodesProjection {
        guard let envelope = store.kernel?.libraryAllEpisodesEnvelope(
            filter: filter.rawValue,
            query: query,
            limit: limit
        ),
              let data = envelope.data(using: .utf8),
              let decoded = try? JSONDecoder.allEpisodesProjection.decode(Response.self, from: data)
        else { return AllEpisodesProjection(episodeIDs: [], totalCount: 0) }
        return AllEpisodesProjection(episodeIDs: decoded.episodeIds, totalCount: decoded.totalCount)
    }

    func episodes(in store: AppStateStore) -> [Episode] {
        episodeIDs.compactMap { store.episode(id: $0) }
    }

    private struct Response: Decodable {
        let episodeIds: [UUID]
        let totalCount: Int
    }
}

private extension JSONDecoder {
    static let allEpisodesProjection: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return decoder
    }()
}
