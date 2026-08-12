import Foundation

/// Resolves fuzzy episode requests against Apple's podcast directory and the
/// publishers' RSS feeds, then imports only the returned matches so the shared
/// core can address them with the same stable IDs as library episodes.
@MainActor
struct PodcastCatalogEpisodeSearchService {
    typealias DirectorySearch = @MainActor (String, Int) async throws -> [ITunesSearchClient.Result]
    typealias FeedLoad = @Sendable (URL) async throws -> ParsedFeed

    struct ParsedFeed: Sendable {
        let podcast: Podcast
        let episodes: [Episode]
    }

    struct Match: Sendable {
        let podcast: Podcast
        let episode: Episode
        let feedURL: URL
        let score: Int
    }

    struct SearchResult: Sendable {
        let episodes: [Episode]
        let boundedResult: String
    }

    enum SearchError: Error {
        case noMatches
    }

    private let directorySearch: DirectorySearch
    private let feedLoad: FeedLoad

    init(
        directorySearch: @escaping DirectorySearch = { term, limit in
            try await ITunesSearchClient.search(term, limit: limit)
        },
        feedLoad: @escaping FeedLoad = Self.loadFeed
    ) {
        self.directorySearch = directorySearch
        self.feedLoad = feedLoad
    }

    func search(
        episodeQuery: String,
        podcastHint: String?,
        limit: Int,
        store: AppStateStore
    ) async throws -> SearchResult {
        let cleanQuery = episodeQuery.trimmingCharacters(in: .whitespacesAndNewlines)
        let cleanHint = podcastHint?.trimmingCharacters(in: .whitespacesAndNewlines)
        let directoryTerm = cleanHint.flatMap { $0.isEmpty ? nil : $0 } ?? cleanQuery
        let shows = try await directorySearch(directoryTerm, 8)
        let feeds = await loadFeeds(for: Array(shows.prefix(8)))
        let matches = Self.rank(
            feeds: feeds,
            episodeQuery: cleanQuery,
            podcastHint: cleanHint,
            limit: max(1, min(limit, 10))
        )
        guard !matches.isEmpty else { throw SearchError.noMatches }

        var rows: [[String: Any]] = []
        var storedEpisodes: [Episode] = []
        for match in matches {
            let stored = try await store.upsertExternalEpisodeAndWait(
                podcastID: Self.stablePodcastID(for: match.feedURL),
                feedURL: match.feedURL,
                podcastTitle: match.podcast.title,
                audioURL: match.episode.enclosureURL,
                guid: match.episode.guid,
                title: match.episode.title,
                description: match.episode.description,
                publishedAt: match.episode.pubDate,
                enclosureMimeType: match.episode.enclosureMimeType,
                imageURL: match.episode.imageURL ?? match.podcast.imageURL,
                duration: match.episode.duration
            )
            rows.append([
                "episode_id": stored.id.uuidString.lowercased(),
                "title": stored.title,
                "podcast": match.podcast.title,
                "published_at": ISO8601DateFormatter().string(from: stored.pubDate),
            ])
            storedEpisodes.append(stored)
        }
        let data = try JSONSerialization.data(withJSONObject: ["episodes": rows])
        return SearchResult(
            episodes: storedEpisodes,
            boundedResult: String(decoding: data, as: UTF8.self)
        )
    }

    static func rank(
        feeds: [ParsedFeed],
        episodeQuery: String,
        podcastHint: String?,
        limit: Int
    ) -> [Match] {
        feeds.flatMap { feed -> [Match] in
            guard let feedURL = feed.podcast.feedURL else { return [] }
            return feed.episodes.compactMap { episode in
                let matchScore = episodeScore(episode, query: episodeQuery)
                guard matchScore > 0 else { return nil }
                return Match(
                    podcast: feed.podcast,
                    episode: episode,
                    feedURL: feedURL,
                    score: matchScore + showScore(feed.podcast, hint: podcastHint)
                )
            }
        }
        .filter { $0.score > 0 }
        .sorted {
            if $0.score != $1.score { return $0.score > $1.score }
            return $0.episode.pubDate > $1.episode.pubDate
        }
        .prefix(max(1, limit))
        .map { $0 }
    }

    private func loadFeeds(for shows: [ITunesSearchClient.Result]) async -> [ParsedFeed] {
        await withTaskGroup(of: ParsedFeed?.self) { group in
            for show in shows {
                guard let feedURL = show.feedURL else { continue }
                group.addTask { [feedLoad] in try? await feedLoad(feedURL) }
            }
            var feeds: [ParsedFeed] = []
            for await feed in group {
                if let feed, feed.podcast.feedURL != nil { feeds.append(feed) }
            }
            return feeds
        }
    }

    private static func loadFeed(_ feedURL: URL) async throws -> ParsedFeed {
        var request = URLRequest(url: feedURL)
        request.timeoutInterval = 15
        request.setValue("application/rss+xml, application/atom+xml, application/xml", forHTTPHeaderField: "Accept")
        let (data, response) = try await URLSession.shared.data(for: request)
        guard data.count <= 10_000_000,
              let http = response as? HTTPURLResponse,
              (200..<300).contains(http.statusCode)
        else { throw URLError(.badServerResponse) }
        let parsed = try RSSParser().parse(data: data, feedURL: response.url ?? feedURL)
        return ParsedFeed(podcast: parsed.podcast, episodes: parsed.episodes)
    }

    private static func episodeScore(_ episode: Episode, query: String) -> Int {
        let normalizedQuery = normalize(query)
        guard !normalizedQuery.isEmpty else { return 0 }
        let title = normalize(episode.title)
        let description = normalize(episode.description)
        let queryTokens = Set(tokens(normalizedQuery))
        let titleMatches = queryTokens.intersection(tokens(title)).count
        let descriptionMatches = queryTokens.intersection(tokens(description)).count
        let phraseBonus = title.contains(normalizedQuery) || normalizedQuery.contains(title) ? 80 : 0
        return phraseBonus + titleMatches * 18 + descriptionMatches * 3
    }

    private static func showScore(_ podcast: Podcast, hint: String?) -> Int {
        guard let hint, !hint.isEmpty else { return 0 }
        let normalizedHint = normalize(hint)
        let title = normalize(podcast.title)
        let author = normalize(podcast.author)
        let hintTokens = Set(tokens(normalizedHint))
        let matches = hintTokens.intersection(tokens(title + " " + author)).count
        return (title.contains(normalizedHint) ? 100 : 0) + matches * 20
    }

    private static func normalize(_ value: String) -> String {
        value.folding(options: [.caseInsensitive, .diacriticInsensitive], locale: .current)
            .unicodeScalars
            .map { CharacterSet.alphanumerics.contains($0) ? Character(String($0)) : " " }
            .reduce(into: "") { $0.append($1) }
            .split(whereSeparator: \.isWhitespace)
            .joined(separator: " ")
    }

    private static func tokens(_ value: String) -> Set<String> {
        let ignored: Set<String> = ["a", "an", "and", "for", "from", "in", "of", "on", "the", "to", "with"]
        return Set(value.split(separator: " ").map(String.init).filter { !ignored.contains($0) })
    }

    private static func stablePodcastID(for feedURL: URL) -> UUID {
        let bytes = Array(feedURL.absoluteString.lowercased().utf8)
        var first: UInt64 = 14_695_981_039_346_656_037
        var second: UInt64 = 7_809_847_782_465_536_322
        for byte in bytes {
            first = (first ^ UInt64(byte)) &* 1_099_511_628_211
            second = (second ^ UInt64(byte &+ 31)) &* 1_099_511_628_211
        }
        var value = withUnsafeBytes(of: first.bigEndian, Array.init)
        value.append(contentsOf: withUnsafeBytes(of: second.bigEndian, Array.init))
        value[6] = (value[6] & 0x0F) | 0x50
        value[8] = (value[8] & 0x3F) | 0x80
        return UUID(uuid: (
            value[0], value[1], value[2], value[3], value[4], value[5], value[6], value[7],
            value[8], value[9], value[10], value[11], value[12], value[13], value[14], value[15]
        ))
    }
}
