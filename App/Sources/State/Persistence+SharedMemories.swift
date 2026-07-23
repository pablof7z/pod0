import Foundation

enum LegacyAgentMemoryRetirementError: Error {
    case sourceChanged
}

extension Persistence {
    func activateSharedMemoryAuthority() {
        sharedArtifactAuthority.withLock { $0.memories = true }
    }

    /// SharedLibraryBootstrap owns the persistence migration lock while this
    /// exact source snapshot is retired before Rust authority commits.
    func retireLegacyAgentMemorySource(
        state: AppState,
        matching backup: LegacyAgentMemoryBackup
    ) throws -> Bool {
        let memories = state.agentMemories.sorted { $0.id.uuidString < $1.id.uuidString }
        guard memories.isEmpty || memories == backup.memories,
              state.compiledMemory == nil || state.compiledMemory == backup.compiled
        else { throw LegacyAgentMemoryRetirementError.sourceChanged }
        if memories.isEmpty, state.compiledMemory == nil { return true }

        let nextGeneration = max(state.persistenceGeneration, backup.persistenceGeneration)
            .saturatingIncrementedForMemoryCutover
        var retired = state
        retired.persistenceGeneration = nextGeneration
        retired.agentMemories = []
        retired.compiledMemory = nil
        let metadata = try Self.memoryMetadataEncoder.encode(metadataState(from: retired))
        try episodeStore.commitMetadata(metadata, generation: nextGeneration)
        revision.withLock { $0 = max($0, nextGeneration) }
        lastWrittenRevision.withLock { $0 = max($0, nextGeneration) }
        return true
    }

    private static let memoryMetadataEncoder: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        return encoder
    }()
}

private extension UInt64 {
    var saturatingIncrementedForMemoryCutover: UInt64 {
        self == .max ? .max : self + 1
    }
}
