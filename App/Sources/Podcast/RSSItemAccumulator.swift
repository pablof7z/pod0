import Foundation

/// Mutable scratch state for a single `<item>` while `RSSParser` walks its
/// children. Lives outside the parser file to keep `RSSParser.swift` under the
/// 300-line soft cap.
///
/// `internal` so the parser delegate (in the same module) can build it; not
/// part of the public API surface.
struct RSSItemAccumulator {
    /// Stable floor date for missing/malformed pubDate. Keeps broken feeds
    /// from appearing as "new now" on every refresh.
    static let fallbackPubDate = Date(timeIntervalSince1970: 0)

    var title: String = ""
    var description: String = ""
    var pubDateRaw: String?
    var guid: String?
    var duration: TimeInterval?
    var enclosureURL: URL?
    var enclosureMimeType: String?
    var itunesImageURL: URL?

    var preferredTranscript: (url: URL, kind: TranscriptKind?)?
    var chaptersURL: URL?

    var persons: [Episode.Person] = []
    var soundBites: [Episode.SoundBite] = []

    var pendingPerson: Episode.Person?
    var pendingSoundBiteStart: TimeInterval?
    var pendingSoundBiteDuration: TimeInterval?

    /// Returns `nil` when the item lacks an `<enclosure>` URL — common in
    /// hybrid blog/podcast feeds and not playable.
    func makeEpisode(podcastID: UUID) -> Episode? {
        guard let enclosureURL else { return nil }

        let resolvedGUID: String = {
            if let g = guid, !g.isEmpty { return g }
            return RSSParser.synthesizedGUID(
                enclosure: enclosureURL,
                pubDateRaw: pubDateRaw
            )
        }()

        let parsedDate: Date = pubDateRaw.flatMap(DateParsing.parseRFC822)
            ?? Self.fallbackPubDate

        return Episode(
            podcastID: podcastID,
            guid: resolvedGUID,
            title: title.trimmingCharacters(in: .whitespacesAndNewlines),
            description: description,
            pubDate: parsedDate,
            duration: duration,
            enclosureURL: enclosureURL,
            enclosureMimeType: enclosureMimeType,
            imageURL: itunesImageURL,
            chapters: nil,
            persons: persons.isEmpty ? nil : persons,
            soundBites: soundBites.isEmpty ? nil : soundBites,
            publisherTranscriptURL: preferredTranscript?.url,
            publisherTranscriptType: preferredTranscript?.kind,
            chaptersURL: chaptersURL
        )
    }

    /// Higher rank wins. JSON > VTT > SRT > HTML > text > unknown.
    static func transcriptRank(_ kind: TranscriptKind?) -> Int {
        switch kind {
        case .json: return 5
        case .vtt: return 4
        case .srt: return 3
        case .html: return 2
        case .text: return 1
        case nil: return 0
        }
    }
}
