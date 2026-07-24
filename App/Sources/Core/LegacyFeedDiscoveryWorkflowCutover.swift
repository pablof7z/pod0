import Foundation
import Pod0Core

enum LegacyFeedDiscoveryWorkflowCutoverError: Error {
    case verificationFailed
}

enum LegacyFeedDiscoveryWorkflowCutover {
    @MainActor
    static func run(
        facade: Pod0Facade,
        state: AppState,
        jobStore: JobStore,
        backupRoot: URL
    ) throws {
        var report = facade.feedDiscoveryCutover()
        if report.stage == .authoritative {
            try finishAuthoritativeRetirement(
                report: report,
                state: state,
                jobStore: jobStore,
                backupRoot: backupRoot
            )
            return
        }

        let snapshot: LegacyFeedDiscoveryWorkflowSnapshot
        switch report.stage {
        case .notStarted:
            snapshot = try .capture(
                state: state,
                jobStore: jobStore
            )
            let inspection = inspect(
                facade: facade,
                snapshot: snapshot
            )
            guard inspection.stage == .notStarted,
                  inspection.failure == nil,
                  let generation = inspection.sourceGeneration,
                  inspection.backupDigest == snapshot.backupDigest,
                  inspection.backupByteCount == snapshot.backupByteCount
            else { throw LegacyFeedDiscoveryWorkflowCutoverError.verificationFailed }
            _ = try snapshot.backup.publish(
                to: backupRoot,
                sourceGeneration: generation
            )
            report = stage(facade: facade, snapshot: snapshot)
        case .staged:
            snapshot = try restore(report: report, state: state, backupRoot: backupRoot)
        case .authoritative, .blocked:
            throw LegacyFeedDiscoveryWorkflowCutoverError.verificationFailed
        }

        guard let generation = report.sourceGeneration,
              report.stage == .staged,
              report.failure == nil,
              report.backupDigest == snapshot.backupDigest,
              report.backupByteCount == snapshot.backupByteCount,
              report.inspectedJobCount == UInt32(snapshot.backup.jobs.count),
              report.candidateCount == UInt32(snapshot.candidates.count),
              report.blockedCount == snapshot.blockedCount
        else { throw LegacyFeedDiscoveryWorkflowCutoverError.verificationFailed }

        guard try jobStore.retireLegacyFeedDiscovery(
            matching: snapshot.backup,
            sourceGeneration: generation,
            sourceDigest: snapshot.backupDigest.stableString
        ), try jobStore.legacyFeedDiscoverySourceIsRetired()
        else { throw LegacyFeedDiscoveryWorkflowCutoverError.verificationFailed }

        report = facade.commitLegacyFeedDiscoveryCutover(sourceGeneration: generation)
        guard report.stage == .authoritative,
              report.sourceGeneration == generation,
              try jobStore.legacyFeedDiscoverySourceIsRetired()
        else { throw LegacyFeedDiscoveryWorkflowCutoverError.verificationFailed }
    }
}

private extension LegacyFeedDiscoveryWorkflowCutover {
    static func inspect(
        facade: Pod0Facade,
        snapshot: LegacyFeedDiscoveryWorkflowSnapshot
    ) -> LegacyFeedDiscoveryCutoverProjection {
        facade.inspectLegacyFeedDiscoveryCutover(
            backupDigest: snapshot.backupDigest,
            backupByteCount: snapshot.backupByteCount,
            notificationsEnabled: snapshot.backup.notificationsEnabled,
            inspectedJobCount: UInt32(snapshot.backup.jobs.count),
            blockedCount: snapshot.blockedCount,
            candidates: snapshot.candidates
        )
    }

    static func stage(
        facade: Pod0Facade,
        snapshot: LegacyFeedDiscoveryWorkflowSnapshot
    ) -> LegacyFeedDiscoveryCutoverProjection {
        facade.stageLegacyFeedDiscoveryCutover(
            backupDigest: snapshot.backupDigest,
            backupByteCount: snapshot.backupByteCount,
            notificationsEnabled: snapshot.backup.notificationsEnabled,
            inspectedJobCount: UInt32(snapshot.backup.jobs.count),
            blockedCount: snapshot.blockedCount,
            candidates: snapshot.candidates
        )
    }

    static func restore(
        report: LegacyFeedDiscoveryCutoverProjection,
        state: AppState,
        backupRoot: URL
    ) throws -> LegacyFeedDiscoveryWorkflowSnapshot {
        guard let generation = report.sourceGeneration else {
            throw LegacyFeedDiscoveryWorkflowCutoverError.verificationFailed
        }
        let backup = try LegacyFeedDiscoveryWorkflowBackup.load(
            from: backupRoot,
            sourceGeneration: generation,
            expectedDigest: report.backupDigest,
            expectedByteCount: report.backupByteCount
        )
        return try .restore(backup, state: state)
    }

    static func finishAuthoritativeRetirement(
        report: LegacyFeedDiscoveryCutoverProjection,
        state: AppState,
        jobStore: JobStore,
        backupRoot: URL
    ) throws {
        guard let generation = report.sourceGeneration,
              let digest = report.backupDigest,
              let byteCount = report.backupByteCount
        else { throw LegacyFeedDiscoveryWorkflowCutoverError.verificationFailed }
        let backup = try LegacyFeedDiscoveryWorkflowBackup.load(
            from: backupRoot,
            sourceGeneration: generation,
            expectedDigest: digest,
            expectedByteCount: byteCount
        )
        _ = try LegacyFeedDiscoveryWorkflowSnapshot.restore(backup, state: state)
        guard try jobStore.retireLegacyFeedDiscovery(
            matching: backup,
            sourceGeneration: generation,
            sourceDigest: digest.stableString
        ), try jobStore.legacyFeedDiscoverySourceIsRetired()
        else { throw LegacyFeedDiscoveryWorkflowCutoverError.verificationFailed }
    }
}
