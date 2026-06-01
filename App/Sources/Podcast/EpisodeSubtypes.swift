import Foundation

// MARK: - Episode Podcasting 2.0 substructs
//
// Split out of `Episode.swift` to keep that file under the 500-line AGENTS.md
// hard limit. These are the value types embedded on `Episode` for Podcasting
// 2.0 enrichment (persons, soundbites), ad detection, and agent-generation
// provenance. `Episode.Chapter` stays in `Episode.swift` (it is the most
// frequently referenced substruct, kept next to the chapter helpers).

extension Episode {
    /// `<podcast:person>` — host / guest / cohost / etc.
    /// See podcasting2.org/docs/podcast-namespace/tags/person.
    struct Person: Codable, Sendable, Hashable, Identifiable {
        var id: UUID
        /// Display name (element text).
        var name: String
        /// `role` attribute: host, guest, cohost, …. Stored verbatim
        /// (case-insensitive); lower-cased for comparison.
        var role: String?
        /// `group` attribute (e.g. cast, writing). Optional.
        var group: String?
        /// `img` attribute — headshot URL.
        var imageURL: URL?
        /// `href` attribute — link to the person's homepage / social.
        var linkURL: URL?

        init(
            id: UUID = UUID(),
            name: String,
            role: String? = nil,
            group: String? = nil,
            imageURL: URL? = nil,
            linkURL: URL? = nil
        ) {
            self.id = id
            self.name = name
            self.role = role
            self.group = group
            self.imageURL = imageURL
            self.linkURL = linkURL
        }
    }

    /// `<podcast:soundbite>` — a short, publisher-curated highlight clip.
    struct SoundBite: Codable, Sendable, Hashable, Identifiable {
        var id: UUID
        /// `startTime` attribute, seconds.
        var startTime: TimeInterval
        /// `duration` attribute, seconds.
        var duration: TimeInterval
        /// Optional element text — a human-friendly title.
        var title: String?

        init(
            id: UUID = UUID(),
            startTime: TimeInterval,
            duration: TimeInterval,
            title: String? = nil
        ) {
            self.id = id
            self.startTime = startTime
            self.duration = duration
            self.title = title
        }
    }

    /// A detected ad span inside the audio. Produced by `AIChapterCompiler`
    /// from the transcript and persisted on the episode so the player can
    /// auto-skip (gated by `Settings.autoSkipAds`) and the chapter rail can
    /// flag overlapping chapters with the amber stripe.
    struct AdSegment: Codable, Sendable, Hashable, Identifiable {
        var id: UUID
        /// Start of the ad in seconds from the beginning of the episode.
        var start: TimeInterval
        /// End of the ad in seconds. Always greater than `start`.
        var end: TimeInterval
        /// Where in the episode this ad sits — pre-roll, mid-roll, or
        /// post-roll. Drives the "Skip 30s ad" pre-roll affordance.
        var kind: AdKind

        init(
            id: UUID = UUID(),
            start: TimeInterval,
            end: TimeInterval,
            kind: AdKind
        ) {
            self.id = id
            self.start = start
            self.end = end
            self.kind = kind
        }

        private enum CodingKeys: String, CodingKey {
            case id, start, end, kind
        }

        init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            id = try c.decodeIfPresent(UUID.self, forKey: .id) ?? UUID()
            start = try c.decode(TimeInterval.self, forKey: .start)
            end = try c.decode(TimeInterval.self, forKey: .end)
            kind = try c.decodeIfPresent(AdKind.self, forKey: .kind) ?? .midroll
        }
    }

    /// Classification for an `AdSegment`. `preroll` ads anchor the
    /// "Skip 30s ad" button above the scrubber; `midroll` is the common
    /// case; `postroll` segments are flagged but don't drive the pre-roll UI.
    enum AdKind: String, Codable, Sendable, Hashable, CaseIterable {
        case preroll
        case midroll
        case postroll
    }

    /// Records where an agent-generated episode was commissioned from.
    /// Stored on the episode so the player can surface a tappable source link.
    enum GenerationSource: Sendable, Equatable, Hashable {
        case inAppChat(conversationID: UUID)
        case nostr(rootEventID: String, peerPubkeyHex: String)
    }
}

// MARK: - Episode.GenerationSource Codable

extension Episode.GenerationSource: Codable {
    private enum CodingKeys: String, CodingKey {
        case type, conversationID, rootEventID, peerPubkeyHex
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let type = try c.decode(String.self, forKey: .type)
        switch type {
        case "inAppChat":
            let id = try c.decode(UUID.self, forKey: .conversationID)
            self = .inAppChat(conversationID: id)
        case "nostr":
            let rootEventID = try c.decode(String.self, forKey: .rootEventID)
            let peerPubkeyHex = try c.decode(String.self, forKey: .peerPubkeyHex)
            self = .nostr(rootEventID: rootEventID, peerPubkeyHex: peerPubkeyHex)
        default:
            throw DecodingError.dataCorrupted(.init(
                codingPath: [CodingKeys.type],
                debugDescription: "Unknown GenerationSource type: \(type)"
            ))
        }
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .inAppChat(let conversationID):
            try c.encode("inAppChat", forKey: .type)
            try c.encode(conversationID, forKey: .conversationID)
        case .nostr(let rootEventID, let peerPubkeyHex):
            try c.encode("nostr", forKey: .type)
            try c.encode(rootEventID, forKey: .rootEventID)
            try c.encode(peerPubkeyHex, forKey: .peerPubkeyHex)
        }
    }
}
