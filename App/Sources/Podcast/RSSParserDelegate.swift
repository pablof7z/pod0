import Foundation

/// `XMLParserDelegate` for `RSSParser`. Lifted out so `RSSParser.swift`
/// stays under the 300-line soft cap.
///
/// `internal` (not file-private) so `RSSParser` in a sibling file can
/// instantiate it. The class is created once per `parse(...)` call, never
/// escapes, and never crosses concurrency domains — `RSSParser` itself is
/// `Sendable`.
final class RSSParserDelegate: NSObject, XMLParserDelegate {
    let podcastID: UUID
    private let feedURL: URL

    // Channel-level accumulated state
    var sawChannel: Bool = false
    var channelTitle: String = ""
    var channelAuthor: String = ""
    var channelDescription: String = ""
    var channelLanguage: String = ""
    var channelImageURL: URL?
    var channelCategories: [String] = []

    // Item-level accumulated state (current item being built)
    private var inItem: Bool = false
    private var item: RSSItemAccumulator = RSSItemAccumulator()

    // Element stack + text buffer
    private var elementStack: [String] = []
    private var textBuffer: String = ""

    // Channel <image><url> nesting flag
    private var inChannelImage: Bool = false

    // Output
    var episodes: [Episode] = []

    init(podcastID: UUID, feedURL: URL) {
        self.podcastID = podcastID
        self.feedURL = feedURL
    }

    // MARK: XMLParserDelegate

    func parser(
        _ parser: XMLParser,
        didStartElement elementName: String,
        namespaceURI: String?,
        qualifiedName qName: String?,
        attributes attributeDict: [String: String] = [:]
    ) {
        elementStack.append(elementName)
        textBuffer = ""

        switch elementName {
        case "channel":
            sawChannel = true

        case "item":
            inItem = true
            item = RSSItemAccumulator()

        case "image":
            // <channel><image><url>…</url></image>
            if !inItem { inChannelImage = true }

        case "enclosure":
            if inItem {
                if let urlString = attributeDict["url"], let url = resolvedURL(urlString) {
                    item.enclosureURL = url
                }
                item.enclosureMimeType = attributeDict["type"]
            }

        case "itunes:image":
            if let href = attributeDict["href"], let url = resolvedURL(href) {
                if inItem { item.itunesImageURL = url }
                else { channelImageURL = channelImageURL ?? url }
            }

        case "itunes:category":
            if !inItem, let text = attributeDict["text"], !text.isEmpty {
                if !channelCategories.contains(text) {
                    channelCategories.append(text)
                }
            }

        case "podcast:transcript":
            if inItem, let urlString = attributeDict["url"], let url = resolvedURL(urlString) {
                let kind = TranscriptKind.from(mimeType: attributeDict["type"])
                let currentRank = RSSItemAccumulator.transcriptRank(item.preferredTranscript?.kind)
                let proposedRank = RSSItemAccumulator.transcriptRank(kind)
                if item.preferredTranscript == nil || proposedRank > currentRank {
                    item.preferredTranscript = (url: url, kind: kind)
                }
            }

        case "podcast:chapters":
            if inItem, let urlString = attributeDict["url"], let url = resolvedURL(urlString) {
                item.chaptersURL = url
            }

        case "podcast:person":
            if inItem {
                item.pendingPerson = Episode.Person(
                    name: "",
                    role: attributeDict["role"],
                    group: attributeDict["group"],
                    imageURL: attributeDict["img"].flatMap { resolvedURL($0) },
                    linkURL: attributeDict["href"].flatMap { resolvedURL($0) }
                )
            }

        case "podcast:soundbite":
            if inItem,
               let startStr = attributeDict["startTime"],
               let durStr = attributeDict["duration"],
               let start = TimeInterval(startStr),
               let dur = TimeInterval(durStr) {
                item.pendingSoundBiteStart = start
                item.pendingSoundBiteDuration = dur
            }

        default:
            break
        }
    }

    func parser(_ parser: XMLParser, foundCharacters string: String) {
        textBuffer.append(string)
    }

    func parser(_ parser: XMLParser, foundCDATA CDATABlock: Data) {
        if let s = String(data: CDATABlock, encoding: .utf8) {
            textBuffer.append(s)
        }
    }

    func parser(
        _ parser: XMLParser,
        didEndElement elementName: String,
        namespaceURI: String?,
        qualifiedName qName: String?
    ) {
        defer {
            if !elementStack.isEmpty { elementStack.removeLast() }
            textBuffer = ""
        }
        let raw = textBuffer
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)

        switch elementName {

        // MARK: Channel-level

        case "title" where !inItem:
            // Avoid clobbering with <image><title>.
            if !inChannelImage, channelTitle.isEmpty { channelTitle = trimmed }

        case "description" where !inItem:
            if channelDescription.isEmpty { channelDescription = trimmed }

        case "itunes:summary" where !inItem:
            if channelDescription.isEmpty { channelDescription = trimmed }

        case "language" where !inItem:
            channelLanguage = trimmed

        case "itunes:author" where !inItem:
            if channelAuthor.isEmpty { channelAuthor = trimmed }

        case "url" where inChannelImage:
            if let url = resolvedURL(trimmed) { channelImageURL = channelImageURL ?? url }

        case "image" where !inItem:
            inChannelImage = false

        // MARK: Item-level

        case "title" where inItem:
            item.title = trimmed

        case "description" where inItem:
            if item.description.isEmpty { item.description = raw }

        case "itunes:summary" where inItem:
            if item.description.isEmpty { item.description = raw }

        case "content:encoded" where inItem:
            // Richer description. Always preferred over plain <description>.
            item.description = raw

        case "pubDate" where inItem:
            item.pubDateRaw = trimmed

        case "guid" where inItem:
            item.guid = trimmed

        case "itunes:duration" where inItem:
            item.duration = parseDuration(trimmed)

        case "podcast:person" where inItem:
            if var person = item.pendingPerson {
                person.name = trimmed
                if !person.name.isEmpty {
                    item.persons.append(person)
                }
                item.pendingPerson = nil
            }

        case "podcast:soundbite" where inItem:
            if let start = item.pendingSoundBiteStart,
               let dur = item.pendingSoundBiteDuration {
                let title = trimmed.isEmpty ? nil : trimmed
                item.soundBites.append(
                    Episode.SoundBite(startTime: start, duration: dur, title: title)
                )
            }
            item.pendingSoundBiteStart = nil
            item.pendingSoundBiteDuration = nil

        case "item":
            if let episode = item.makeEpisode(podcastID: podcastID) {
                episodes.append(episode)
            }
            inItem = false
            item = RSSItemAccumulator()

        default:
            break
        }
    }

    // MARK: Helpers

    /// Parses iTunes durations: `H:MM:SS`, `MM:SS`, or raw seconds.
    private func parseDuration(_ s: String) -> TimeInterval? {
        guard !s.isEmpty else { return nil }
        let parts = s.split(separator: ":").map(String.init)
        if parts.count == 1 {
            return TimeInterval(parts[0])
        }
        var seconds: TimeInterval = 0
        for part in parts {
            guard let value = Double(part) else { return nil }
            seconds = seconds * 60 + value
        }
        return seconds
    }

    private func resolvedURL(_ raw: String) -> URL? {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        if trimmed.hasPrefix("//") {
            let scheme = feedURL.scheme ?? "https"
            return URL(string: "\(scheme):\(trimmed)")
        }
        if let absolute = URL(string: trimmed), absolute.scheme != nil {
            return absolute
        }
        return URL(string: trimmed, relativeTo: feedURL)?.absoluteURL
    }
}
