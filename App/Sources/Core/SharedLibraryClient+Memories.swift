import Foundation
import Pod0Core

struct SharedMemorySnapshot {
    let collectionRevision: StateRevision
    let memories: [AgentMemory]
    let compiled: CompiledAgentMemory?
    let operations: [OperationProjection]
}

extension SharedLibraryClient {
    func receiveMemories(revision: UInt64) {
        guard revision >= lastMemoriesRevision else { return }
        lastMemoriesRevision = revision
        let snapshot = loadMemoryPages(scope: .all)
        cachedMemories = snapshot
        store?.applySharedMemories(snapshot)
        resolveWaiters(snapshot.operations)
    }

    func updateMemory(_ memory: AgentMemory, content: String) throws {
        _ = try executeMemoryCommand(.updateMemory(
            memoryId: MemoryId(uuid: memory.id),
            expectedMemoryRevision: MemoryRevision(value: memory.revision),
            content: content
        ))
    }

    func createMemory(content: String) throws -> AgentMemory {
        let result = try executeMemoryCommand(.createMemory(content: content))
        guard case .memoryCreated(let memoryID, _, _) = result,
              let id = memoryID.uuid,
              let memory = cachedMemories?.memories.first(where: { $0.id == id })
        else { throw SharedLibraryError.unavailable }
        return memory
    }

    func setMemoryDeleted(_ memory: AgentMemory, deleted: Bool) throws {
        _ = try executeMemoryCommand(.setMemoryDeleted(
            memoryId: MemoryId(uuid: memory.id),
            expectedMemoryRevision: MemoryRevision(value: memory.revision),
            deleted: deleted
        ))
    }

    func clearMemories() throws {
        let revision = cachedMemories?.collectionRevision
            ?? loadMemoryPages(scope: .all).collectionRevision
        _ = try executeMemoryCommand(.clearMemories(expectedCollectionRevision: revision))
    }

    func loadMemoryPages(scope: MemoryProjectionScope) -> SharedMemorySnapshot {
        var offset: UInt32 = 0
        var collectionRevision = StateRevision(value: 1)
        var memories: [AgentMemory] = []
        var compiled: CompiledAgentMemory?
        var operations: [OperationProjection] = []
        while true {
            let envelope = facade.snapshot(request: ProjectionRequest(
                scope: .memories(scope: scope),
                offset: offset,
                maxItems: 200
            ))
            guard case .memories(let page) = envelope.projection else { break }
            collectionRevision = page.collectionRevision
            memories.append(contentsOf: page.memories.compactMap(\.swiftValue))
            compiled = page.compiled?.swiftValue
            if operations.isEmpty { operations = page.operations }
            guard page.hasMore, offset <= UInt32.max - 200 else { break }
            offset += 200
        }
        return SharedMemorySnapshot(
            collectionRevision: collectionRevision,
            memories: memories,
            compiled: compiled,
            operations: operations
        )
    }

    private func executeMemoryCommand(_ command: ApplicationCommand) throws -> OperationResult? {
        let commandID = CommandId(uuid: UUID())
        facade.dispatch(command: CommandEnvelope(
            commandId: commandID,
            cancellationId: CancellationId(uuid: UUID()),
            expectedRevision: nil,
            command: command
        ))
        let snapshot = loadMemoryPages(scope: .all)
        cachedMemories = snapshot
        store?.applySharedMemories(snapshot)
        guard let operation = snapshot.operations.first(where: { $0.commandId == commandID })
        else { throw SharedLibraryError.unavailable }
        switch operation.stage {
        case .succeeded:
            return operation.result
        case .failed, .cancelled, .unsupported:
            throw SharedLibraryError(operation.failure?.code)
        case .accepted, .running, .blocked:
            throw SharedLibraryError.unavailable
        }
    }
}
