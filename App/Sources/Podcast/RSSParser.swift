import Foundation

/// Pure-Foundation RSS 2.0 parser with iTunes + Podcasting 2.0 namespace
/// support. No networking — `FeedClient` does the I/O and hands bytes here.
///
/// Coverage (see `docs/spec/research/transcription-stack.md` §2 and the
/// Podcasting 2.0 namespace spec):
///
/// - Base RSS 2.0: `<channel>` (title, description, language, image, link),
///   `<item>` (title, description, pubDate, guid, enclosure, link).
/// - iTunes: `<itunes:author>`, `<itunes:summary>`, `<itunes:image href>`,
///   `<itunes:duration>`, `<itunes:category text>`.
/// - Podcasting 2.0: `<podcast:transcript url type>`,
///   `<podcast:chapters url>`, `<podcast:person role group img href>`,
///   `<podcast:soundbite startTime duration>`.
///   `<podcast:value>` and `<podcast:location>` are tolerated (not yet
///   exploded into structured fields; Lane 6 / Lane 9 own those).
///
/// Returned values are `Sendable`; the underlying `XMLParser` delegate is a
/// class held only for the duration of the parse call and never escapes.
/// That keeps Swift 6 strict concurrency happy.
struct RSSParser: Sendable {

    /// Parsed feed result. `podcast` carries channel-level fields with `id`
    /// set to the caller-provided `podcastID` so callers can chain straight
    /// into persistence.
    struct ParsedFeed: Sendable {
        var podcast: Podcast
        var episodes: [Episode]
    }

    enum ParseError: Error, LocalizedError, Sendable {
        case malformedXML(underlying: String)
        case missingChannel
        case missingFeedURL

        /// User-facing copy. Plain prose, no Swift case names or NSError
        /// codes — those leak straight into the Add Show error label
        /// otherwise (the previous `String(describing:)` rendering printed
        /// `malformedXML(underlying: "NSXMLParserErrorDomain error 111")`
        /// to the user, which means nothing to anyone who isn't a Swift
        /// developer).
        var errorDescription: String? {
            switch self {
            case .malformedXML:
                return "This URL doesn't look like a podcast feed. " +
                    "The server returned something other than valid RSS — " +
                    "double-check the URL and try again."
            case .missingChannel:
                return "The feed is missing its <channel> element, so " +
                    "there's nothing to subscribe to."
            case .missingFeedURL:
                return "The feed didn't include its own URL, so we can't " +
                    "fetch updates for it later."
            }
        }
    }

    /// Parses raw feed bytes. The caller supplies the canonical `feedURL`
    /// (RSS rarely echoes its own URL) and a stable `podcastID` (either an
    /// existing podcast's id or a freshly minted one for a new subscribe).
    func parse(
        data: Data,
        feedURL: URL,
        podcastID: UUID = UUID()
    ) throws -> ParsedFeed {
        let delegate = RSSParserDelegate(
            podcastID: podcastID,
            feedURL: feedURL
        )
        let parser = XMLParser(data: data)
        parser.delegate = delegate
        parser.shouldProcessNamespaces = false
        parser.shouldReportNamespacePrefixes = false

        guard parser.parse() else {
            let underlying = parser.parserError?.localizedDescription ?? "unknown XMLParser error"
            throw ParseError.malformedXML(underlying: underlying)
        }
        guard delegate.sawChannel else { throw ParseError.missingChannel }

        let podcast = Podcast(
            id: podcastID,
            kind: .rss,
            feedURL: feedURL,
            title: delegate.channelTitle.rss_trimmedOrEmpty,
            author: delegate.channelAuthor.rss_trimmedOrEmpty,
            imageURL: delegate.channelImageURL,
            description: delegate.channelDescription.rss_trimmedOrEmpty,
            language: delegate.channelLanguage.rss_nilIfBlank,
            categories: delegate.channelCategories,
            discoveredAt: Date(),
            lastRefreshedAt: Date()
        )
        return ParsedFeed(podcast: podcast, episodes: delegate.episodes)
    }

    /// Synthesizes a deterministic GUID for items missing `<guid>`. Combines
    /// the enclosure URL and the parsed pubDate (or raw string) so a re-fetch
    /// produces the same id. Lane 6 keys embeddings off `Episode.guid`, so
    /// stability is load-bearing.
    static func synthesizedGUID(enclosure: URL?, pubDateRaw: String?) -> String {
        let enclosurePart = enclosure?.absoluteString ?? "no-enclosure"
        let datePart = pubDateRaw?.trimmingCharacters(in: .whitespacesAndNewlines) ?? "no-date"
        return "synth::\(enclosurePart)::\(datePart)"
    }
}

// MARK: - String helpers (also used by `RSSParserDelegate`)

extension String {
    var rss_trimmedOrEmpty: String {
        self.trimmingCharacters(in: .whitespacesAndNewlines)
    }
    var rss_nilIfBlank: String? {
        let t = self.trimmingCharacters(in: .whitespacesAndNewlines)
        return t.isEmpty ? nil : t
    }
}

// Companions:
//   `RSSParserDelegate` — XMLParserDelegate impl.
//   `RSSItemAccumulator` — per-item scratch state.
//   `DateParsing` — RFC 822 / ISO 8601 cascade. (`RSSDateParsing.swift`)
