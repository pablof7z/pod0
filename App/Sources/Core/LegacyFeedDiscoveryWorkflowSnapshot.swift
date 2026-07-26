import Foundation
import Pod0Core

struct LegacyFeedDiscoveryWorkflowSnapshot {
    let backup: LegacyFeedDiscoveryWorkflowBackup
    let backupDigest: ContentDigest
    let backupByteCount: UInt64
    let candidates: [LegacyFeedDiscoveryCandidateInput]
    let blockedCount: UInt32

    static func capture(
        state: AppState,
        jobStore: JobStore,
        now: Date = Date()
    ) throws -> Self {
        let backup = LegacyFeedDiscoveryWorkflowBackup(
            formatVersion: 1,
            persistenceGeneration: state.persistenceGeneration,
            capturedAt: now,
            notificationsEnabled: state.settings.legacyNotifyOnNewEpisodes,
            jobs: try jobStore.legacyFeedDiscoveryJobs(),
            artifacts: try jobStore.legacyFeedDiscoveryArtifacts()
        )
        return try restore(backup, state: state)
    }

    static func restore(
        _ backup: LegacyFeedDiscoveryWorkflowBackup,
        state: AppState
    ) throws -> Self {
        let mapped = try LegacyFeedDiscoveryWorkflowMapper.map(
            backup: backup,
            state: state,
            now: backup.capturedAt
        )
        let evidence = try backup.evidence()
        return Self(
            backup: backup,
            backupDigest: evidence.digest,
            backupByteCount: evidence.byteCount,
            candidates: mapped.candidates,
            blockedCount: mapped.blockedCount
        )
    }
}
