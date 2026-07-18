import Foundation

/// Minimal NIP-01 event value used for ephemeral Blossom upload authorization.
struct NostrEventDraft: Sendable, Equatable {
    var kind: Int
    var content: String
    var tags: [[String]]
    var createdAt: Int

    init(
        kind: Int,
        content: String,
        tags: [[String]] = [],
        createdAt: Int = Int(Date().timeIntervalSince1970)
    ) {
        self.kind = kind
        self.content = content
        self.tags = tags
        self.createdAt = createdAt
    }
}

struct SignedNostrEvent: Sendable, Equatable, Codable {
    let id: String
    let pubkey: String
    let created_at: Int
    let kind: Int
    let tags: [[String]]
    let content: String
    let sig: String
}
