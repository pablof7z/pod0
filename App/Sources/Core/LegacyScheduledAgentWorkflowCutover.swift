import Foundation
import Pod0Core

enum LegacyScheduledAgentWorkflowCutoverError: Error {
    case verificationFailed
}

enum LegacyScheduledAgentWorkflowCutover {
    @MainActor
    static func run(
        facade: Pod0Facade,
        persistence: Persistence,
        state: AppState,
        jobStore: JobStore,
        history: LegacyChatHistorySource,
        backupRoot: URL
    ) throws {
        var report = facade.scheduledAgentCutover()
        if report.stage == .authoritative {
            try finishAuthoritativeRetirement(
                report: report,
                persistence: persistence,
                state: state,
                jobStore: jobStore,
                backupRoot: backupRoot
            )
            persistence.activateSharedScheduledAgentAuthority()
            return
        }

        let snapshot: LegacyScheduledAgentWorkflowSnapshot
        switch report.stage {
        case .notStarted:
            snapshot = try .capture(state: state, jobStore: jobStore, history: history)
            let inspection = facade.inspectLegacyScheduledAgentCutover(
                backupDigest: snapshot.backupDigest,
                backupByteCount: snapshot.backupByteCount,
                tasks: snapshot.tasks,
                occurrences: snapshot.occurrences
            )
            guard inspection.stage == .notStarted,
                  inspection.failure == nil,
                  let generation = inspection.sourceGeneration,
                  inspection.backupDigest == snapshot.backupDigest,
                  inspection.backupByteCount == snapshot.backupByteCount
            else { throw LegacyScheduledAgentWorkflowCutoverError.verificationFailed }
            _ = try snapshot.backup.publish(
                to: backupRoot,
                sourceGeneration: generation
            )
            report = facade.stageLegacyScheduledAgentCutover(
                backupDigest: snapshot.backupDigest,
                backupByteCount: snapshot.backupByteCount,
                tasks: snapshot.tasks,
                occurrences: snapshot.occurrences
            )
        case .staged, .verified:
            snapshot = try restore(report: report, backupRoot: backupRoot)
        case .authoritative, .blocked:
            throw LegacyScheduledAgentWorkflowCutoverError.verificationFailed
        }

        guard let generation = report.sourceGeneration,
              report.failure == nil,
              report.backupDigest == snapshot.backupDigest,
              report.backupByteCount == snapshot.backupByteCount,
              Int(report.taskCount) == snapshot.tasks.count,
              Int(report.occurrenceCount) == snapshot.occurrences.count
        else { throw LegacyScheduledAgentWorkflowCutoverError.verificationFailed }

        if report.stage == .staged {
            report = facade.verifyLegacyScheduledAgentCutover(sourceGeneration: generation)
        }
        guard report.stage == .verified,
              report.sourceGeneration == generation,
              try persistence.retireLegacyScheduledAgentSource(
                  state: state,
                  matching: snapshot.backup
              ),
              try jobStore.legacyScheduledAgentSourceIsRetired()
        else { throw LegacyScheduledAgentWorkflowCutoverError.verificationFailed }

        report = facade.commitLegacyScheduledAgentCutover(sourceGeneration: generation)
        guard report.stage == .authoritative,
              report.sourceGeneration == generation else {
            throw LegacyScheduledAgentWorkflowCutoverError.verificationFailed
        }
        persistence.activateSharedScheduledAgentAuthority()
    }
}

private extension LegacyScheduledAgentWorkflowCutover {
    static func restore(
        report: LegacyScheduledAgentCutoverProjection,
        backupRoot: URL
    ) throws -> LegacyScheduledAgentWorkflowSnapshot {
        guard let generation = report.sourceGeneration else {
            throw LegacyScheduledAgentWorkflowCutoverError.verificationFailed
        }
        let backup = try LegacyScheduledAgentWorkflowBackup.load(
            from: backupRoot,
            sourceGeneration: generation,
            expectedDigest: report.backupDigest,
            expectedByteCount: report.backupByteCount
        )
        return try .restore(backup)
    }

    static func finishAuthoritativeRetirement(
        report: LegacyScheduledAgentCutoverProjection,
        persistence: Persistence,
        state: AppState,
        jobStore: JobStore,
        backupRoot: URL
    ) throws {
        guard let generation = report.sourceGeneration else {
            throw LegacyScheduledAgentWorkflowCutoverError.verificationFailed
        }
        if let digest = report.backupDigest, let bytes = report.backupByteCount {
            let backup = try LegacyScheduledAgentWorkflowBackup.load(
                from: backupRoot,
                sourceGeneration: generation,
                expectedDigest: digest,
                expectedByteCount: bytes
            )
            guard try persistence.retireLegacyScheduledAgentSource(
                state: state,
                matching: backup
            ) else { throw LegacyScheduledAgentWorkflowCutoverError.verificationFailed }
        } else if !state.agentScheduledTasks.isEmpty {
            throw LegacyScheduledAgentWorkflowCutoverError.verificationFailed
        }
        guard try jobStore.legacyScheduledAgentSourceIsRetired() else {
            throw LegacyScheduledAgentWorkflowCutoverError.verificationFailed
        }
    }
}
