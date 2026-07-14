import Foundation

/// Parses an OPML 2.0 subscription list into seeded `Podcast` records
/// ready for first refresh.
///
/// The OPML returned by Apple Podcasts, Pocket Casts, Overcast, Castro etc.
/// shares a common skeleton:
///
/// ```xml
/// <opml version="2.0">
///   <body>
///     <outline text="Podcasts">
///       <outline type="rss" text="Tim Ferriss"
///                xmlUrl="https://feeds.example/timferriss.rss"
///                htmlUrl="https://example/timferriss"
///                description="Author of …" />
///       …
///     </outline>
///   </body>
/// </opml>
/// ```
///
/// We seed `PodcastSubscription` from `xmlUrl` (the only required attribute)
/// plus whatever metadata is present. Episodes are *not* fetched here — Lane 3
/// queues the new subscriptions through `FeedClient` after import.
struct OPMLImport: Sendable {

    enum OPMLError: Error, LocalizedError, Sendable {
        case malformedXML(underlying: String)

        /// User-facing copy. `OPMLImportSheet` reads
        /// `error.localizedDescription` directly into the inline error
        /// label — without a `LocalizedError` conformance the user would
        /// see `malformedXML(underlying: "NSXMLParserErrorDomain error 5")`
        /// (Apple's `Error.localizedDescription` falls back to a raw
        /// `String(describing:)` for unconformed enums). Same trap that
        /// previously hit `RSSParser.ParseError`.
        var errorDescription: String? {
            switch self {
            case .malformedXML:
                return "This file isn't a valid OPML export. " +
                    "If you exported it from another podcast app, " +
                    "check that the export completed and try again."
            }
        }
    }

    /// Parses raw OPML bytes. Outline nodes without an `xmlUrl` attribute are
    /// skipped (they are typically grouping folders). Order is preserved as
    /// emitted by the source app, so the Library import sheet can render the
    /// list as the user already knows it.
    func parseOPML(data: Data) throws -> [Podcast] {
        let delegate = OPMLImportDelegate()
        let parser = XMLParser(data: data)
        parser.delegate = delegate
        parser.shouldProcessNamespaces = false
        parser.shouldReportNamespacePrefixes = false
        guard parser.parse() else {
            let underlying = parser.parserError?.localizedDescription ?? "unknown XMLParser error"
            throw OPMLError.malformedXML(underlying: underlying)
        }
        return delegate.podcasts
    }
}

private final class OPMLImportDelegate: NSObject, XMLParserDelegate {
    var podcasts: [Podcast] = []
    private var seenFeedURLs: Set<URL> = []

    func parser(
        _ parser: XMLParser,
        didStartElement elementName: String,
        namespaceURI: String?,
        qualifiedName qName: String?,
        attributes attributeDict: [String: String] = [:]
    ) {
        guard elementName == "outline" else { return }
        // Per OPML 2.0 §6, podcast feeds carry `type="rss"` and `xmlUrl`; we
        // also accept `type="link"` plus `xmlUrl` (Castro emits this).
        guard let xmlUrlString = attributeDict["xmlUrl"],
              let feedURL = URL(string: xmlUrlString) else {
            return
        }
        guard !seenFeedURLs.contains(feedURL) else { return }
        seenFeedURLs.insert(feedURL)

        let title = attributeDict["text"]
            ?? attributeDict["title"]
            ?? feedURL.host
            ?? feedURL.absoluteString
        let description = attributeDict["description"] ?? ""
        let language = attributeDict["language"]

        let podcast = Podcast(
            kind: .rss,
            feedURL: feedURL,
            title: title,
            description: description,
            language: language
        )
        podcasts.append(podcast)
    }
}
