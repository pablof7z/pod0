import Foundation

// MARK: - Anchor
// Polymorphic reference target — links notes to their context.
// Discriminated union serialized as { "kind": "...", "id": "..." } for JSON round-trip.

enum Anchor: Codable, Hashable, Sendable {
    case note(id: UUID)
    /// A note attached directly to a Friend.
    case friend(id: UUID)
    /// A note anchored to a specific moment in an episode.
    case episode(id: UUID, positionSeconds: TimeInterval)

    private enum Kind: String, Codable { case note, friend, episode }
    private enum CodingKeys: String, CodingKey { case kind, id, positionSeconds }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        switch try c.decode(Kind.self, forKey: .kind) {
        case .note:   self = .note(id: try c.decode(UUID.self, forKey: .id))
        case .friend: self = .friend(id: try c.decode(UUID.self, forKey: .id))
        case .episode:
            let id  = try c.decode(UUID.self, forKey: .id)
            let pos = (try? c.decodeIfPresent(TimeInterval.self, forKey: .positionSeconds)) ?? 0
            self = .episode(id: id, positionSeconds: pos)
        }
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .note(let id):
            try c.encode(Kind.note, forKey: .kind)
            try c.encode(id, forKey: .id)
        case .friend(let id):
            try c.encode(Kind.friend, forKey: .kind)
            try c.encode(id, forKey: .id)
        case .episode(let id, let pos):
            try c.encode(Kind.episode, forKey: .kind)
            try c.encode(id, forKey: .id)
            try c.encode(pos, forKey: .positionSeconds)
        }
    }
}
