import Foundation

/// Thin Swift decoder over Rust-owned Apple Podcasts directory discovery.
///
/// Rust owns endpoint shape, HTTP capability dispatch, storefront/top-chart
/// lookup, result ordering, and Apple JSON parsing. Swift keeps this row type
/// only so the Add Show UI can render native controls without knowing the
/// kernel's wire DTO.
enum ITunesSearchClient {

    struct Result: Decodable, Sendable, Hashable, Identifiable {
        let collectionId: Int
        let collectionName: String
        let artistName: String?
        let feedUrl: String?
        let artworkUrl600: String?
        let artworkUrl100: String?
        let primaryGenreName: String?
        let trackCount: Int?

        var id: Int { collectionId }

        var feedURL: URL? { feedUrl.flatMap { URL(string: $0) } }

        /// Prefer 600px artwork. Rust already falls back to the 100px tile
        /// when Apple omits 600px, so both slots point at the same canonical
        /// artwork URL for kernel-backed rows.
        var artworkURL: URL? {
            if let s = artworkUrl600, let u = URL(string: s) { return u }
            if let s = artworkUrl100, let u = URL(string: s) { return u }
            return nil
        }
    }

    static func search(
        _ term: String,
        kernel: KernelModel?,
        limit: Int = 25
    ) async throws -> [Result] {
        guard let kernel else { throw ITunesSearchError.unavailable("KernelModel") }
        let envelope = await MainActor.run {
            kernel.itunesDirectorySearchEnvelope(
                query: term,
                type: PodcastDirectorySearchType.podcast.rawValue,
                limit: limit
            )
        }
        guard let envelope else { throw ITunesSearchError.unavailable("KernelModel") }
        return try decodeSearchEnvelope(envelope)
    }

    static func topPodcasts(
        kernel: KernelModel?,
        limit: Int = 25,
        storefront: String = "us"
    ) async throws -> [Result] {
        guard let kernel else { throw ITunesSearchError.unavailable("KernelModel") }
        let envelope = await MainActor.run {
            kernel.itunesTopPodcastsEnvelope(limit: limit, storefront: storefront)
        }
        guard let envelope else { throw ITunesSearchError.unavailable("KernelModel") }
        return try decodeSearchEnvelope(envelope)
    }

    private static func decodeSearchEnvelope(_ envelope: String) throws -> [Result] {
        guard let data = envelope.data(using: .utf8) else {
            throw ITunesSearchError.parseError("Directory search returned non-UTF8 data")
        }
        let decoded = try JSONDecoder().decode(DirectorySearchEnvelope.self, from: data)
        if let error = decoded.error {
            throw ITunesSearchError.parseError(error)
        }
        return (decoded.result ?? []).compactMap(\.result)
    }

    private struct DirectorySearchEnvelope: Decodable {
        var result: [DirectoryHitDTO]?
        var error: String?
    }

    private struct DirectoryHitDTO: Decodable {
        var collectionID: Int?
        var podcastTitle: String
        var author: String?
        var feedURL: String?
        var artworkURL: String?
        var primaryGenreName: String?
        var trackCount: Int?

        private enum CodingKeys: String, CodingKey {
            case collectionID = "collection_id"
            case podcastTitle = "podcast_title"
            case author
            case feedURL = "feed_url"
            case artworkURL = "artwork_url"
            case primaryGenreName = "primary_genre_name"
            case trackCount = "track_count"
        }

        var result: Result? {
            guard let collectionID else { return nil }
            return Result(
                collectionId: collectionID,
                collectionName: podcastTitle,
                artistName: author,
                feedUrl: feedURL,
                artworkUrl600: artworkURL,
                artworkUrl100: artworkURL,
                primaryGenreName: primaryGenreName,
                trackCount: trackCount
            )
        }
    }
}

private enum ITunesSearchError: LocalizedError {
    case parseError(String)
    case unavailable(String)

    var errorDescription: String? {
        switch self {
        case .parseError(let message): return "Directory search error: \(message)"
        case .unavailable(let name): return "\(name) is unavailable."
        }
    }
}
