import Foundation

/// A product-level comment anchor. It deliberately cannot express protocol
/// tags; the typed NMP boundary tracked by pablof7z/nmp#572 must own those.
enum CommentTarget: Codable, Hashable, Sendable {
    case episode(guid: String)

    private enum CodingKeys: String, CodingKey {
        case type
        case guid
    }

    private enum Kind: String, Codable {
        case episode
    }

    init(from decoder: any Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        switch try values.decode(Kind.self, forKey: .type) {
        case .episode:
            self = .episode(guid: try values.decode(String.self, forKey: .guid))
        }
    }

    func encode(to encoder: any Encoder) throws {
        var values = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .episode(let guid):
            try values.encode(Kind.episode, forKey: .type)
            try values.encode(guid, forKey: .guid)
        }
    }
}

/// A presentation value accepted only from a future verified typed provider.
struct EpisodeComment: Identifiable, Hashable, Sendable {
    let id: String
    let target: CommentTarget
    let authorPubkeyHex: String
    let content: String
    let createdAt: Date

    var authorShortKey: String {
        guard authorPubkeyHex.count > 8 else { return authorPubkeyHex }
        return "\(authorPubkeyHex.prefix(4))…\(authorPubkeyHex.suffix(4))"
    }
}
