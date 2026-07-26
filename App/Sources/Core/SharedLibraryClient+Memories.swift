import Foundation
import Pod0Core

struct SharedMemorySnapshot: @unchecked Sendable {
    let collectionRevision: StateRevision
    let memories: [AgentMemory]
    let compiled: CompiledAgentMemory?
    let operations: [OperationProjection]
}

extension SharedLibraryClient {
    func receiveMemories(revision: UInt64) {
        guard revision >= lastMemoriesRevision else { return }
        lastMemoriesRevision = revision
        let facade = facade
        memoryProjectionTask?.cancel()
        memoryProjectionTask = Task { @MainActor [weak self] in
            let snapshot = await Task.detached(priority: .utility) {
                Self.loadMemoryPages(facade: facade, scope: .all)
            }.value
            guard !Task.isCancelled, let self, revision == lastMemoriesRevision else { return }
            cachedMemories = snapshot
            store?.applySharedMemories(snapshot)
            resolveWaiters(snapshot.operations)
        }
    }

    func updateMemory(_ memory: AgentMemory, content: String) async throws {
        _ = try await execute(.updateMemory(
            memoryId: MemoryId(uuid: memory.id),
            expectedMemoryRevision: MemoryRevision(value: memory.revision),
            content: content
        ))
        _ = await refreshMemorySnapshot()
    }

    func createMemory(content: String) async throws -> AgentMemory {
        let result = try await execute(.createMemory(content: content))
        let snapshot = await refreshMemorySnapshot()
        guard case .memoryCreated(let memoryID, _, _) = result,
              let id = memoryID.uuid,
              let memory = snapshot.memories.first(where: { $0.id == id })
        else { throw SharedLibraryError.unavailable }
        return memory
    }

    func setMemoryDeleted(_ memory: AgentMemory, deleted: Bool) async throws {
        _ = try await execute(.setMemoryDeleted(
            memoryId: MemoryId(uuid: memory.id),
            expectedMemoryRevision: MemoryRevision(value: memory.revision),
            deleted: deleted
        ))
        _ = await refreshMemorySnapshot()
    }

    func clearMemories() async throws {
        let revision = await memoryCollectionRevision()
        _ = try await execute(.clearMemories(expectedCollectionRevision: revision))
        _ = await refreshMemorySnapshot()
    }

    nonisolated static func loadMemoryPages(
        facade: Pod0Facade,
        scope: MemoryProjectionScope
    ) -> SharedMemorySnapshot {
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

    private func memoryCollectionRevision() async -> StateRevision {
        if let revision = cachedMemories?.collectionRevision { return revision }
        let facade = facade
        let snapshot = await Task.detached(priority: .utility) {
            Self.loadMemoryPages(facade: facade, scope: .all)
        }.value
        cachedMemories = snapshot
        store?.applySharedMemories(snapshot)
        return snapshot.collectionRevision
    }

    private func refreshMemorySnapshot() async -> SharedMemorySnapshot {
        let facade = facade
        let snapshot = await Task.detached(priority: .utility) {
            Self.loadMemoryPages(facade: facade, scope: .all)
        }.value
        cachedMemories = snapshot
        store?.applySharedMemories(snapshot)
        return snapshot
    }
}
