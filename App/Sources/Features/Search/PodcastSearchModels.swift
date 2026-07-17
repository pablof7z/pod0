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

struct PodcastTranscriptSearchHit: Identifiable, Hashable, Sendable {
    var chunk: Chunk
    var score: Float
    var snippet: String
    var id: UUID { chunk.id }
}

struct PodcastWikiSearchHit: Identifiable, Hashable, Sendable {
    var page: WikiPage
    var excerpt: String
    var score: Int
    var id: UUID { page.id }
}

enum PodcastSearchEngine {
    static func localResults(
        query: String,
        state: AppState,
        limit: Int = 8
    ) -> PodcastLocalSearchResults {
        let trimmed = query.trimmed
        guard !trimmed.isEmpty else { return PodcastLocalSearchResults() }
        let tokens = tokenize(trimmed)
        // Local search covers only the user's followed RSS podcasts —
        // synthetic shows (Agent Generated, Unknown) don't surface here.
        let followedPodcastIDs = Set(state.subscriptions.map(\.podcastID))
        let followedPodcasts = state.podcasts.filter {
            followedPodcastIDs.contains($0.id) && $0.kind == .rss
        }
        let podcastsByID = Dictionary(
            uniqueKeysWithValues: followedPodcasts.map { ($0.id, $0) }
        )

        let shows = followedPodcasts.compactMap { podcast -> PodcastShowSearchHit? in
            let score = score(
                fields: [
                    (podcast.title, 8),
                    (podcast.author, 4),
                    (podcast.description, 2),
                    (podcast.categories.joined(separator: " "), 2)
                ],
                query: trimmed,
                tokens: tokens
            )
            guard score > 0 else { return nil }
            return PodcastShowSearchHit(podcast: podcast, score: score)
        }
        .sorted(by: ranked)
        .prefix(limit)

        let episodes = state.episodes.compactMap { episode -> PodcastEpisodeSearchHit? in
            guard let podcast = podcastsByID[episode.podcastID] else { return nil }
            let people = (episode.persons ?? []).map(\.name).joined(separator: " ")
            let soundBites = (episode.soundBites ?? []).compactMap(\.title).joined(separator: " ")
            let fields = [
                (episode.title, 8),
                (podcast.title, 4),
                (people, 3),
                (soundBites, 3),
                (episode.plainTextSummary, 2)
            ]
            let score = score(fields: fields, query: trimmed, tokens: tokens)
            guard score > 0 else { return nil }
            return PodcastEpisodeSearchHit(
                episode: episode,
                podcast: podcast,
                snippet: bestSnippet(fields.map(\.0), query: trimmed, tokens: tokens),
                score: score
            )
        }
        .sorted {
            if $0.score != $1.score { return $0.score > $1.score }
            return $0.episode.pubDate > $1.episode.pubDate
        }
        .prefix(limit)

        return PodcastLocalSearchResults(shows: Array(shows), episodes: Array(episodes))
    }

    static func wikiResults(
        query: String,
        pages: [WikiPage],
        limit: Int = 8
    ) -> [PodcastWikiSearchHit] {
        let trimmed = query.trimmed
        guard !trimmed.isEmpty else { return [] }
        let tokens = tokenize(trimmed)
        return pages.compactMap { page -> PodcastWikiSearchHit? in
            let claims = page.allClaims.map(\.text)
            let score = score(
                fields: [(page.title, 8), (page.summary, 4)] + claims.map { ($0, 3) },
                query: trimmed,
                tokens: tokens
            )
            guard score > 0 else { return nil }
            return PodcastWikiSearchHit(
                page: page,
                excerpt: bestSnippet([page.summary] + claims, query: trimmed, tokens: tokens),
                score: score
            )
        }
        .sorted {
            if $0.score != $1.score { return $0.score > $1.score }
            return $0.page.generatedAt > $1.page.generatedAt
        }
        .prefix(limit)
        .map { $0 }
    }

    private static func ranked(_ lhs: PodcastShowSearchHit, _ rhs: PodcastShowSearchHit) -> Bool {
        if lhs.score != rhs.score { return lhs.score > rhs.score }
        return lhs.podcast.title.localizedCaseInsensitiveCompare(rhs.podcast.title) == .orderedAscending
    }

    private static func tokenize(_ query: String) -> [String] {
        query
            .lowercased()
            .split { !$0.isLetter && !$0.isNumber }
            .map(String.init)
            .filter { $0.count >= 2 }
    }

    private static func score(
        fields: [(String, Int)],
        query: String,
        tokens: [String]
    ) -> Int {
        let needle = query.lowercased()
        return fields.reduce(into: 0) { total, field in
            let haystack = field.0.lowercased()
            guard !haystack.isBlank else { return }
            if haystack == needle { total += field.1 * 8 }
            if haystack.contains(needle) { total += field.1 * 4 }
            for token in tokens where haystack.contains(token) {
                total += field.1
            }
        }
    }

    private static func bestSnippet(_ fields: [String], query: String, tokens: [String]) -> String {
        let cleaned = fields.map(cleanSnippet).filter { !$0.isEmpty }
        let needle = query.lowercased()
        if let exact = cleaned.first(where: { $0.lowercased().contains(needle) }) {
            return exact
        }
        return cleaned.max { lhs, rhs in
            tokenHits(lhs, tokens: tokens) < tokenHits(rhs, tokens: tokens)
        } ?? ""
    }

    private static func tokenHits(_ text: String, tokens: [String]) -> Int {
        let lower = text.lowercased()
        return tokens.filter { lower.contains($0) }.count
    }

    private static func cleanSnippet(_ text: String) -> String {
        // Pure UTF-16 scanning via `CharacterSet` — the prior shape
        // compiled `\\s+` via `.regularExpression` on every call, and
        // this runs N times per search result while the user types.
        text.components(separatedBy: .whitespacesAndNewlines)
            .filter { !$0.isEmpty }
            .joined(separator: " ")
    }
}
