import Foundation

// MARK: - Agent Memory

struct AgentMemory: Identifiable, Hashable, Sendable {
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

}

// MARK: - Compiled Agent Memory

/// LLM-consolidated summary of the active `AgentMemory` set. Regenerated
/// by the retired Swift agent before shared-core cutover.
/// Idempotency guard: `sourceMemoryIDs` is the exact ordered set of active
/// memory ids folded into this compile — if the current `agentMemories`
/// id sequence (filtered to active, sorted by `createdAt`) matches, no
/// recompile is needed.
struct CompiledAgentMemory: Hashable, Sendable {
    var text: String
    var compiledAt: Date
    var sourceMemoryCount: Int
    var sourceMemoryIDs: [UUID]
}
