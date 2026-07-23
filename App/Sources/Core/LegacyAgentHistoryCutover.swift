import Foundation
import Pod0Core

enum LegacyAgentHistoryCutoverError: Error {
    case verificationFailed
}

enum LegacyAgentHistoryCutover {
    static func run(
        facade: Pod0Facade,
        source: LegacyChatHistorySource,
        backupRoot: URL
    ) throws {
        var report = facade.agentHistoryCutover()
        if report.stage == .authoritative {
            try finishRetirement(report: report, source: source, backupRoot: backupRoot)
            return
        }

        let backup: LegacyAgentHistoryBackup
        let mapped: [LegacyAgentHistoryConversationInput]
        switch report.stage {
        case .notStarted:
            backup = LegacyAgentHistoryBackup(conversations: source.conversations)
            mapped = try LegacyAgentHistoryMapper.map(backup)
            let evidence = try backup.evidence()
            let inspection = facade.inspectLegacyAgentHistoryCutover(
                backupDigest: evidence.digest,
                backupByteCount: evidence.byteCount,
                conversations: mapped
            )
            guard inspection.stage == .notStarted,
                  inspection.failure == nil,
                  let generation = inspection.sourceGeneration,
                  inspection.backupDigest == evidence.digest,
                  inspection.backupByteCount == evidence.byteCount
            else { throw LegacyAgentHistoryCutoverError.verificationFailed }
            try backup.publish(to: backupRoot, sourceGeneration: generation)
            report = facade.stageLegacyAgentHistoryCutover(
                backupDigest: evidence.digest,
                backupByteCount: evidence.byteCount,
                conversations: mapped
            )
        case .staged, .verified:
            backup = try restore(report: report, backupRoot: backupRoot)
            mapped = try LegacyAgentHistoryMapper.map(backup)
        case .authoritative, .blocked:
            throw LegacyAgentHistoryCutoverError.verificationFailed
        }

        let backupEvidence = try backup.evidence()
        let mappedTurnCount = mapped.reduce(0) { count, conversation in
            count + conversation.turns.count
        }
        guard let generation = report.sourceGeneration,
              report.failure == nil,
              let backupDigest = report.backupDigest,
              let backupByteCount = report.backupByteCount,
              backupEvidence.digest == backupDigest,
              backupEvidence.byteCount == backupByteCount,
              Int(report.conversationCount) == mapped.count,
              Int(report.turnCount) == mappedTurnCount
        else { throw LegacyAgentHistoryCutoverError.verificationFailed }

        if report.stage == .staged {
            report = facade.verifyLegacyAgentHistoryCutover(sourceGeneration: generation)
        }
        guard report.stage == .verified else {
            throw LegacyAgentHistoryCutoverError.verificationFailed
        }
        try source.retire(matching: backup.conversations)
        guard source.isRetired else {
            throw LegacyAgentHistoryCutoverError.verificationFailed
        }
        report = facade.commitLegacyAgentHistoryCutover(sourceGeneration: generation)
        guard report.stage == .authoritative else {
            throw LegacyAgentHistoryCutoverError.verificationFailed
        }
    }
}

private extension LegacyAgentHistoryCutover {
    static func restore(
        report: LegacyAgentHistoryCutoverProjection,
        backupRoot: URL
    ) throws -> LegacyAgentHistoryBackup {
        guard let generation = report.sourceGeneration else {
            throw LegacyAgentHistoryCutoverError.verificationFailed
        }
        return try LegacyAgentHistoryBackup.load(
            from: backupRoot,
            sourceGeneration: generation,
            expectedDigest: report.backupDigest,
            expectedByteCount: report.backupByteCount
        )
    }

    static func finishRetirement(
        report: LegacyAgentHistoryCutoverProjection,
        source: LegacyChatHistorySource,
        backupRoot: URL
    ) throws {
        if source.isRetired { return }
        let backup = try restore(report: report, backupRoot: backupRoot)
        try source.retire(matching: backup.conversations)
        guard source.isRetired else {
            throw LegacyAgentHistoryCutoverError.verificationFailed
        }
    }
}
