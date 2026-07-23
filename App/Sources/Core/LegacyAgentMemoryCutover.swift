import Foundation
import Pod0Core

enum LegacyAgentMemoryCutoverError: Error {
    case verificationFailed
}

enum LegacyAgentMemoryCutover {
    static func run(
        facade: Pod0Facade,
        persistence: Persistence,
        state: AppState,
        backupRoot: URL
    ) throws {
        var report = facade.memoryCutover()
        if report.stage == .authoritative {
            persistence.activateSharedMemoryAuthority()
            return
        }

        let backup: LegacyAgentMemoryBackup
        let mappedMemories: [LegacyMemoryInput]
        let mappedCompiled: LegacyCompiledMemoryInput?
        switch report.stage {
        case .notStarted:
            backup = LegacyAgentMemoryBackup(state: state)
            (mappedMemories, mappedCompiled) = map(backup)
            let evidence = try backup.evidence()
            let inspection = facade.inspectLegacyMemoryCutover(
                backupDigest: evidence.digest,
                backupByteCount: evidence.byteCount,
                memories: mappedMemories,
                compiled: mappedCompiled
            )
            guard inspection.stage == .notStarted,
                  inspection.failure == nil,
                  let generation = inspection.sourceGeneration,
                  inspection.backupDigest == evidence.digest,
                  inspection.backupByteCount == evidence.byteCount
            else { throw LegacyAgentMemoryCutoverError.verificationFailed }
            try backup.publish(to: backupRoot, sourceGeneration: generation)
            report = facade.stageLegacyMemoryCutover(
                backupDigest: evidence.digest,
                backupByteCount: evidence.byteCount,
                memories: mappedMemories,
                compiled: mappedCompiled
            )
        case .staged, .verified:
            backup = try restore(report: report, backupRoot: backupRoot)
            (mappedMemories, mappedCompiled) = map(backup)
        case .authoritative, .blocked:
            throw LegacyAgentMemoryCutoverError.verificationFailed
        }

        let evidence = try backup.evidence()
        guard let generation = report.sourceGeneration,
              report.failure == nil,
              report.backupDigest == evidence.digest,
              report.backupByteCount == evidence.byteCount,
              Int(report.memoryCount) == mappedMemories.count,
              report.compiledPresent == (mappedCompiled != nil)
        else { throw LegacyAgentMemoryCutoverError.verificationFailed }
        if report.stage == .staged {
            report = facade.verifyLegacyMemoryCutover(sourceGeneration: generation)
        }
        guard report.stage == .verified,
              try persistence.retireLegacyAgentMemorySource(state: state, matching: backup)
        else { throw LegacyAgentMemoryCutoverError.verificationFailed }
        report = facade.commitLegacyMemoryCutover(sourceGeneration: generation)
        guard report.stage == .authoritative else {
            throw LegacyAgentMemoryCutoverError.verificationFailed
        }
        persistence.activateSharedMemoryAuthority()
    }
}

private extension LegacyAgentMemoryCutover {
    static func restore(
        report: LegacyMemoryCutoverProjection,
        backupRoot: URL
    ) throws -> LegacyAgentMemoryBackup {
        guard let generation = report.sourceGeneration else {
            throw LegacyAgentMemoryCutoverError.verificationFailed
        }
        return try LegacyAgentMemoryBackup.load(
            from: backupRoot,
            sourceGeneration: generation,
            expectedDigest: report.backupDigest,
            expectedByteCount: report.backupByteCount
        )
    }

    static func map(
        _ backup: LegacyAgentMemoryBackup
    ) -> ([LegacyMemoryInput], LegacyCompiledMemoryInput?) {
        let memories = backup.memories.map {
            LegacyMemoryInput(
                memoryId: MemoryId(uuid: $0.id),
                content: $0.content,
                createdAt: UnixTimestampMilliseconds(date: $0.createdAt),
                deleted: $0.deleted
            )
        }
        let compiled = backup.compiled.map {
            LegacyCompiledMemoryInput(
                text: $0.text,
                compiledAt: UnixTimestampMilliseconds(date: $0.compiledAt),
                sourceMemoryIds: $0.sourceMemoryIDs.map(MemoryId.init(uuid:))
            )
        }
        return (memories, compiled)
    }
}
