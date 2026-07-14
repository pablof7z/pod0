import Foundation

/// Thin client over Apple's free iTunes Search API for podcast discovery.
///
/// No auth, no rate-limit headers in practice for an end-user device. We
/// use the public `entity=podcast` search; the response gives us everything
/// we need to render a result row (artwork URL, title, author, genre,
/// episode count) plus the canonical feed URL we hand to
/// `SubscriptionService.addSubscription` on tap.
///
/// Endpoint: <https://itunes.apple.com/search?media=podcast&entity=podcast&term=…>
enum ITunesSearchClient {

    /// One row in the search response. Decoded with the JSON keys exactly as
    /// Apple emits them so future fields can be added without a custom
    /// `CodingKeys` mapping.
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

        /// Prefer 600px artwork, fall back to the 100px tile.
        var artworkURL: URL? {
            if let s = artworkUrl600, let u = URL(string: s) { return u }
            if let s = artworkUrl100, let u = URL(string: s) { return u }
            return nil
        }
    }

    private struct Response: Decodable {
        let results: [Result]
    }

    /// Searches the iTunes podcast directory. Throws the underlying URLError
    /// or a `DecodingError` so the caller can surface a localized message.
    static func search(_ term: String, limit: Int = 25) async throws -> [Result] {
        var components = URLComponents(string: "https://itunes.apple.com/search")!
        components.queryItems = [
            URLQueryItem(name: "media", value: "podcast"),
            URLQueryItem(name: "entity", value: "podcast"),
            URLQueryItem(name: "term", value: term),
            URLQueryItem(name: "limit", value: String(limit)),
        ]
        guard let url = components.url else { throw URLError(.badURL) }

        var request = URLRequest(url: url)
        request.timeoutInterval = 15
        let (data, response) = try await URLSession.shared.data(for: request)
        guard let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode) else {
            throw URLError(.badServerResponse)
        }

        let decoded = try JSONDecoder().decode(Response.self, from: data)
        // Filter on the *parsed* URL, not just the string presence —
        // iTunes occasionally emits a non-nil feedUrl that doesn't
        // round-trip through `URL(string:)` (rare, but seen on a
        // handful of legacy entries). Filtering on `feedURL` instead of
        // `feedUrl` means rows that survive are guaranteed to subscribe;
        // the previous filter let malformed rows render and silently
        // no-op on tap.
        return decoded.results.filter { $0.feedURL != nil }
    }

    /// Top podcasts in the user's storefront. Fetched as a two-step pipeline
    /// because the marketing-tools RSS feed gives us only iTunes IDs:
    ///
    ///   1. RSS feed at `rss.applemarketingtools.com` → ranked list of IDs.
    ///   2. iTunes Lookup API in one batched call → full `Result` rows
    ///      (with feed URL) for every ID.
    ///
    /// Result order preserves the marketing-feed ranking, not the lookup's
    /// arbitrary response order.
    ///
    /// Defaults to the US storefront because that's the largest catalogue
    /// and the only one Apple guarantees daily refresh on. Pass a different
    /// `storefront` (e.g. `"gb"`, `"de"`) when localising.
    static func topPodcasts(
        limit: Int = 25,
        storefront: String = "us"
    ) async throws -> [Result] {
        // Step 1 — top-N IDs from the marketing feed.
        let topURLString = "https://rss.applemarketingtools.com/api/v2/\(storefront)/podcasts/top/\(limit)/podcasts.json"
        guard let topURL = URL(string: topURLString) else { throw URLError(.badURL) }

        var topRequest = URLRequest(url: topURL)
        topRequest.timeoutInterval = 15
        let (topData, topResponse) = try await URLSession.shared.data(for: topRequest)
        guard let httpTop = topResponse as? HTTPURLResponse, (200..<300).contains(httpTop.statusCode) else {
            throw URLError(.badServerResponse)
        }
        let topFeed = try JSONDecoder().decode(TopPodcastsFeed.self, from: topData)
        let rankedIDs = topFeed.feed.results.compactMap { Int($0.id) }
        guard !rankedIDs.isEmpty else { return [] }

        // Step 2 — single batched lookup for full metadata.
        var lookupComponents = URLComponents(string: "https://itunes.apple.com/lookup")!
        lookupComponents.queryItems = [
            URLQueryItem(name: "id", value: rankedIDs.map(String.init).joined(separator: ",")),
            URLQueryItem(name: "entity", value: "podcast"),
        ]
        guard let lookupURL = lookupComponents.url else { throw URLError(.badURL) }

        var lookupRequest = URLRequest(url: lookupURL)
        lookupRequest.timeoutInterval = 15
        let (lookupData, lookupResponse) = try await URLSession.shared.data(for: lookupRequest)
        guard let httpLookup = lookupResponse as? HTTPURLResponse, (200..<300).contains(httpLookup.statusCode) else {
            throw URLError(.badServerResponse)
        }
        let looked = try JSONDecoder().decode(Response.self, from: lookupData).results

        // Reorder to match the ranked feed; drop rows the lookup didn't
        // return or whose feedUrl doesn't parse — same parsed-URL guard
        // as `search(_:limit:)` so a tapped row is always subscribable.
        let byCollectionID = Dictionary(uniqueKeysWithValues: looked.map { ($0.collectionId, $0) })
        return rankedIDs.compactMap { byCollectionID[$0] }.filter { $0.feedURL != nil }
    }

    // MARK: - Top-feed shapes

    private struct TopPodcastsFeed: Decodable {
        let feed: Body

        struct Body: Decodable {
            let results: [Item]
        }

        struct Item: Decodable {
            let id: String
        }
    }
}
