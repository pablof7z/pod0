import Foundation

// MARK: - Anchor
// Polymorphic reference target — links notes to their context.
// Discriminated union serialized as { "kind": "...", "id": "..." } for JSON round-trip.

enum Anchor: Codable, Hashable, Sendable {
    case note(id: UUID)
    /// A note anchored to a specific moment in an episode — an underline in the
    /// text. Survives independently of any clip that happens to span it.
    case episode(id: UUID, positionSeconds: TimeInterval)
    /// A note about a clip as an artifact — writing in the margin beside a
    /// highlight. Carries no position: the clip is already a span, and a
    /// position here could fall outside its own clip once the clip is retimed.
    /// Annotate a moment with `.episode` instead.
    case clip(id: UUID)

    private enum Kind: String, Codable { case note, episode, clip }
    private enum CodingKeys: String, CodingKey { case kind, id, positionSeconds }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        switch try c.decode(Kind.self, forKey: .kind) {
        case .note:   self = .note(id: try c.decode(UUID.self, forKey: .id))
        case .episode:
            let id  = try c.decode(UUID.self, forKey: .id)
            let pos = (try? c.decodeIfPresent(TimeInterval.self, forKey: .positionSeconds)) ?? 0
            self = .episode(id: id, positionSeconds: pos)
        case .clip:   self = .clip(id: try c.decode(UUID.self, forKey: .id))
        }
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .note(let id):
            try c.encode(Kind.note, forKey: .kind)
            try c.encode(id, forKey: .id)
        case .episode(let id, let pos):
            try c.encode(Kind.episode, forKey: .kind)
            try c.encode(id, forKey: .id)
            try c.encode(pos, forKey: .positionSeconds)
        case .clip(let id):
            try c.encode(Kind.clip, forKey: .kind)
            try c.encode(id, forKey: .id)
        }
    }
}
