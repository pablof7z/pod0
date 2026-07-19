import Foundation

// MARK: - Note

enum NoteKind: String, Codable, Hashable, Sendable {
    case free
    case reflection
    case systemEvent
}

/// Native presentation value projected from the shared core. Codable remains
/// for legacy import and user export; production persistence strips this cache
/// after the Rust notes cutover.
struct Note: Codable, Identifiable, Hashable, Sendable {
    var id: UUID
    var revision: UInt64
    var text: String
    var kind: NoteKind
    var target: Anchor?
    var createdAt: Date
    var deleted: Bool
    /// Who authored this note. Drives the publish-layer wiring contract from
    /// `docs/spec/briefs/identity-05-synthesis.md` §5: `.user` notes sign and
    /// publish with the user identity, `.agent` notes stay local-only.
    var author: NoteAuthor
    var evidence: NoteEvidence?

    init(text: String, kind: NoteKind = .free, target: Anchor? = nil, author: NoteAuthor = .user) {
        self.id = UUID()
        self.revision = 1
        self.text = text
        self.kind = kind
        self.target = target
        self.createdAt = Date()
        self.deleted = false
        self.author = author
        self.evidence = nil
    }

    init(
        id: UUID,
        revision: UInt64,
        text: String,
        kind: NoteKind,
        target: Anchor?,
        createdAt: Date,
        deleted: Bool,
        author: NoteAuthor,
        evidence: NoteEvidence?
    ) {
        self.id = id
        self.revision = revision
        self.text = text
        self.kind = kind
        self.target = target
        self.createdAt = createdAt
        self.deleted = deleted
        self.author = author
        self.evidence = evidence
    }

    private enum CodingKeys: String, CodingKey {
        case id, revision, text, kind, target, createdAt, deleted, author, evidence
    }

    // Forward-compat: every field decoded with `decodeIfPresent` so adding
    // new fields never breaks decode of older persisted state.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decodeIfPresent(UUID.self, forKey: .id) ?? UUID()
        revision = try c.decodeIfPresent(UInt64.self, forKey: .revision) ?? 1
        text = try c.decodeIfPresent(String.self, forKey: .text) ?? ""
        kind = try c.decodeIfPresent(NoteKind.self, forKey: .kind) ?? .free
        target = try c.decodeIfPresent(Anchor.self, forKey: .target)
        createdAt = try c.decodeIfPresent(Date.self, forKey: .createdAt) ?? Date()
        deleted = try c.decodeIfPresent(Bool.self, forKey: .deleted) ?? false
        // Legacy snapshots (pre-NoteAuthor) default to `.user` — they were all
        // user-authored in practice; the agent path is new in this slice.
        author = try c.decodeIfPresent(NoteAuthor.self, forKey: .author) ?? .user
        evidence = try c.decodeIfPresent(NoteEvidence.self, forKey: .evidence)
    }
}

struct NoteEvidence: Codable, Hashable, Sendable {
    let generationID: String
    let transcriptVersionID: String
    let transcriptContentDigest: String
    let spanID: String
}
