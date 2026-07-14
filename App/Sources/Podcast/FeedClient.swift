import Foundation

/// Fetches a `Podcast`'s feed and parses it into an updated podcast +
/// new episodes.
///
/// Conditional GET: sends `If-None-Match` (ETag) and `If-Modified-Since`
/// (Last-Modified) when the podcast has them. A `304 Not Modified` is
/// reported via `FeedFetchResult.notModified` so callers can short-circuit
/// without redoing parser / dedupe work — see baseline §2 (manual + scheduled
/// refresh) and `transcription-stack.md` §5 for the upstream pipeline.
///
/// Pure I/O — no SwiftUI, no `AppStateStore` writes. Lane 1 (audio engine) and
/// Lane 3 (Library) are the consumers.
struct FeedClient: Sendable {

    /// What `fetch(_:)` returned for one feed.
    enum FeedFetchResult: Sendable {
        /// Server returned 304 Not Modified; nothing to parse, but record the
        /// `lastRefreshedAt` so the UI knows we checked.
        case notModified(lastRefreshedAt: Date)
        /// Server returned a fresh feed body; the caller receives a refreshed
        /// podcast (with new ETag/Last-Modified) plus the parsed episodes.
        case updated(
            podcast: Podcast,
            episodes: [Episode],
            lastRefreshedAt: Date
        )
    }

    enum FeedFetchError: Error, Sendable {
        case transport(underlying: String)
        case http(status: Int)
        case parse(RSSParser.ParseError)
        case missingFeedURL
    }

    /// Allows tests to swap in a stub session.
    let session: URLSession
    private let parser: RSSParser

    init(session: URLSession = .shared) {
        self.session = session
        self.parser = RSSParser()
    }

    /// Fetches the podcast's feed, honoring conditional-GET.
    ///
    /// Returns the *new* episodes from this fetch — the caller is responsible
    /// for diffing against persistent storage by `Episode.guid`. We do not
    /// retain state here.
    func fetch(_ podcast: Podcast) async throws -> FeedFetchResult {
        guard let feedURL = podcast.feedURL else {
            throw FeedFetchError.missingFeedURL
        }
        var request = URLRequest(url: feedURL)
        request.httpMethod = "GET"
        // RSS feeds should respond fast — a healthy one returns in
        // 1–5s and slow ones in 15–30s. Cap at 30s instead of the
        // default 60s so a hung feed doesn't block the refresh sweep
        // for a full minute; `SubscriptionRefreshService.refreshAll`
        // already runs feeds concurrently in chunks of 4, but a single
        // stuck feed inside a chunk delays the whole chunk's
        // completion under bounded concurrency.
        request.timeoutInterval = 30
        request.setValue(
            "application/rss+xml, application/xml;q=0.9, */*;q=0.8",
            forHTTPHeaderField: "Accept"
        )
        request.setValue("Podcastr/1.0", forHTTPHeaderField: "User-Agent")
        if let etag = podcast.etag, !etag.isEmpty {
            request.setValue(etag, forHTTPHeaderField: "If-None-Match")
        }
        if let lastModified = podcast.lastModified, !lastModified.isEmpty {
            request.setValue(lastModified, forHTTPHeaderField: "If-Modified-Since")
        }

        let data: Data
        let response: URLResponse
        do {
            (data, response) = try await session.data(for: request)
        } catch {
            throw FeedFetchError.transport(underlying: error.localizedDescription)
        }

        let now = Date()
        guard let httpResponse = response as? HTTPURLResponse else {
            // Non-HTTP scheme (file://, etc.). Treat as 200 if data parses.
            return try parseAndAttach(
                data: data,
                podcast: podcast,
                etag: nil,
                lastModified: nil,
                now: now
            )
        }

        switch httpResponse.statusCode {
        case 200:
            let etag = (httpResponse.value(forHTTPHeaderField: "ETag")
                ?? httpResponse.value(forHTTPHeaderField: "Etag"))
            let lastModified = httpResponse.value(forHTTPHeaderField: "Last-Modified")
            return try parseAndAttach(
                data: data,
                podcast: podcast,
                etag: etag,
                lastModified: lastModified,
                now: now
            )
        case 304:
            return .notModified(lastRefreshedAt: now)
        default:
            throw FeedFetchError.http(status: httpResponse.statusCode)
        }
    }

    // MARK: Private

    private func parseAndAttach(
        data: Data,
        podcast: Podcast,
        etag: String?,
        lastModified: String?,
        now: Date
    ) throws -> FeedFetchResult {
        guard let feedURL = podcast.feedURL else {
            throw FeedFetchError.missingFeedURL
        }
        let parsed: RSSParser.ParsedFeed
        do {
            parsed = try parser.parse(
                data: data,
                feedURL: feedURL,
                podcastID: podcast.id
            )
        } catch let parseError as RSSParser.ParseError {
            throw FeedFetchError.parse(parseError)
        }

        // Merge: keep the existing podcast's identity; overwrite editorial
        // fields from the feed; refresh cache headers.
        var merged = podcast
        if !parsed.podcast.title.isEmpty { merged.title = parsed.podcast.title }
        if !parsed.podcast.author.isEmpty { merged.author = parsed.podcast.author }
        if !parsed.podcast.description.isEmpty { merged.description = parsed.podcast.description }
        if let image = parsed.podcast.imageURL { merged.imageURL = image }
        if let language = parsed.podcast.language { merged.language = language }
        if !parsed.podcast.categories.isEmpty {
            merged.categories = parsed.podcast.categories
        }
        merged.etag = etag ?? merged.etag
        merged.lastModified = lastModified ?? merged.lastModified
        merged.lastRefreshedAt = now

        return .updated(
            podcast: merged,
            episodes: parsed.episodes,
            lastRefreshedAt: now
        )
    }
}
