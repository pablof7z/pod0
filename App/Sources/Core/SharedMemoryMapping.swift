import Foundation
import Pod0Core

extension MemoryRecord {
    var swiftValue: AgentMemory? {
        guard let id = memoryId.uuid else { return nil }
        return AgentMemory(
            id: id,
            revision: revision.value,
            content: content,
            createdAt: createdAt.date,
            deleted: deleted
        )
    }
}

extension CompiledMemoryRecord {
    var swiftValue: CompiledAgentMemory {
        CompiledAgentMemory(
            text: text,
            compiledAt: compiledAt.date,
            sourceMemoryCount: sourceMemoryIds.count,
            sourceMemoryIDs: sourceMemoryIds.compactMap(\.uuid)
        )
    }
}
