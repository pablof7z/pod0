import Foundation

enum LegacyAgentActivityRetirementError: Error, Equatable {
    case verificationFailed
}

extension Persistence {
    /// Removes the dormant native Agent activity payload after Rust has
    /// accepted conversation history and memories. The migration lock held by
    /// SharedLibraryBootstrap prevents a concurrent native metadata commit.
    func retireLegacyAgentActivitySource(state: AppState) throws {
        if state.legacyAgentActivity.isEmpty {
            activateLegacyAgentActivityRetirement()
            return
        }

        let nextGeneration = state.persistenceGeneration
            .saturatingIncrementedForAgentActivityRetirement
        var retired = state
        retired.persistenceGeneration = nextGeneration
        retired.legacyAgentActivity = []

        let metadata = try Self.legacyAgentActivityMetadataEncoder.encode(
            metadataState(from: retired)
        )
        do {
            try episodeStore.commitMetadata(metadata, generation: nextGeneration)
        } catch {
            throw LegacyAgentActivityRetirementError.verificationFailed
        }

        let persisted = try load(loadLegacyChapterAdjuncts: false)
        guard persisted.legacyAgentActivity.isEmpty else {
            throw LegacyAgentActivityRetirementError.verificationFailed
        }

        revision.withLock { $0 = max($0, nextGeneration) }
        lastWrittenRevision.withLock { $0 = max($0, nextGeneration) }
        activateLegacyAgentActivityRetirement()
    }

    func activateLegacyAgentActivityRetirement() {
        sharedArtifactAuthority.withLock { $0.legacyAgentActivityRetired = true }
    }

    private static let legacyAgentActivityMetadataEncoder: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        return encoder
    }()
}

private extension UInt64 {
    var saturatingIncrementedForAgentActivityRetirement: UInt64 {
        self == .max ? .max : self + 1
    }
}
