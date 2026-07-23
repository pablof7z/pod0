import Foundation

// MARK: - Agent Memory

struct AgentMemory: Codable, Identifiable, Hashable, Sendable {
    var id: UUID
    var revision: UInt64
    var content: String
    var createdAt: Date
    var deleted: Bool

    init(content: String) {
        self.id = UUID()
        self.revision = 1
        self.content = content
        self.createdAt = Date()
        self.deleted = false
    }

    init(id: UUID, revision: UInt64, content: String, createdAt: Date, deleted: Bool) {
        self.id = id
        self.revision = revision
        self.content = content
        self.createdAt = createdAt
        self.deleted = deleted
    }

    private enum CodingKeys: String, CodingKey {
        case id, revision, content, createdAt, deleted
    }

    init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        id = try values.decode(UUID.self, forKey: .id)
        revision = try values.decodeIfPresent(UInt64.self, forKey: .revision) ?? 1
        content = try values.decode(String.self, forKey: .content)
        createdAt = try values.decode(Date.self, forKey: .createdAt)
        deleted = try values.decodeIfPresent(Bool.self, forKey: .deleted) ?? false
    }
}

// MARK: - Compiled Agent Memory

/// LLM-consolidated summary of the active `AgentMemory` set. Regenerated
/// by the retired Swift agent before shared-core cutover.
/// Idempotency guard: `sourceMemoryIDs` is the exact ordered set of active
/// memory ids folded into this compile — if the current `agentMemories`
/// id sequence (filtered to active, sorted by `createdAt`) matches, no
/// recompile is needed.
struct CompiledAgentMemory: Codable, Hashable, Sendable {
    var text: String
    var compiledAt: Date
    var sourceMemoryCount: Int
    var sourceMemoryIDs: [UUID]
}
