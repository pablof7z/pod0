import Foundation

// MARK: - Agent Memory

struct AgentMemory: Codable, Identifiable, Hashable, Sendable {
    var id: UUID
    var content: String
    var createdAt: Date
    var deleted: Bool

    init(content: String) {
        self.id = UUID()
        self.content = content
        self.createdAt = Date()
        self.deleted = false
    }
}

// MARK: - Compiled Agent Memory

/// LLM-consolidated summary of the active `AgentMemory` set. Regenerated
/// by `AgentMemoryCompiler` after agent turns that recorded a memory.
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
