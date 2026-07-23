import Foundation
import Pod0Core

struct LegacyScheduledAgentWorkflowSnapshot {
    let backup: LegacyScheduledAgentWorkflowBackup
    let backupDigest: ContentDigest
    let backupByteCount: UInt64
    let tasks: [LegacyScheduledAgentTaskInput]
    let occurrences: [LegacyScheduledAgentOccurrenceInput]

    @MainActor
    static func capture(
        state: AppState,
        jobStore: JobStore,
        history: LegacyChatHistorySource
    ) throws -> Self {
        let backup = LegacyScheduledAgentWorkflowBackup(
            formatVersion: 1,
            persistenceGeneration: state.persistenceGeneration,
            defaultModelReference: state.settings.agentInitialModel,
            tasks: state.agentScheduledTasks.sorted { $0.id.uuidString < $1.id.uuidString },
            jobs: try jobStore.legacyScheduledAgentJobs(),
            artifacts: try jobStore.legacyScheduledAgentArtifacts(),
            conversations: history.conversations
                .filter { $0.isScheduledTask }
                .sorted { $0.id.uuidString < $1.id.uuidString }
        )
        return try restore(backup)
    }

    static func restore(_ backup: LegacyScheduledAgentWorkflowBackup) throws -> Self {
        let mapped = try LegacyScheduledAgentWorkflowMapper.map(backup)
        let evidence = try backup.evidence()
        return Self(
            backup: backup,
            backupDigest: evidence.digest,
            backupByteCount: evidence.byteCount,
            tasks: mapped.tasks,
            occurrences: mapped.occurrences
        )
    }
}
