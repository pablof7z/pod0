import CSQLite3
import Foundation

enum LegacyScheduledAgentWorkflowRetirementError: Error {
    case verificationFailed
}

extension Persistence {
    /// Once Rust authority commits, no later metadata save may resurrect the
    /// legacy Swift definitions as a second durable source of truth.
    func activateSharedScheduledAgentAuthority() {
        sharedArtifactAuthority.withLock { $0.scheduledAgents = true }
    }

    /// Atomically retires all three legacy authority components in the shared
    /// episode SQLite file. SharedLibraryBootstrap already owns writeLock and
    /// WorkflowSQLite.databaseLock while this method executes.
    func retireLegacyScheduledAgentSource(
        state: AppState,
        matching backup: LegacyScheduledAgentWorkflowBackup
    ) throws -> Bool {
        let currentTasks = state.agentScheduledTasks.sorted {
            $0.id.uuidString < $1.id.uuidString
        }
        guard currentTasks.isEmpty || currentTasks == backup.tasks else { return false }
        let jobStore = JobStore(fileURL: episodeStore.fileURL)
        if currentTasks.isEmpty, try jobStore.legacyScheduledAgentSourceIsRetired() {
            return true
        }
        let nextGeneration = max(state.persistenceGeneration, backup.persistenceGeneration)
            .saturatingIncremented
        var retiredState = state
        retiredState.persistenceGeneration = nextGeneration
        retiredState.agentScheduledTasks = []
        let metadata = try Self.scheduledAgentMetadataEncoder.encode(
            metadataState(from: retiredState)
        )
        let artifactRepository = ArtifactRepository(fileURL: episodeStore.fileURL)

        let retired = try episodeStore.withDatabase { db in
            try episodeStore.ensureSchema(in: db)
            try jobStore.ensureSchema(db)
            try artifactRepository.ensureSchema(db)
            try WorkflowSQLite.execute("BEGIN IMMEDIATE TRANSACTION", db)
            do {
                let jobs = try jobStore.legacyScheduledAgentJobs(db: db)
                let artifacts = try jobStore.legacyScheduledAgentArtifacts(db: db)
                guard (jobs.isEmpty || jobs == backup.jobs),
                      (artifacts.isEmpty || artifacts == backup.artifacts) else {
                    try WorkflowSQLite.execute("COMMIT TRANSACTION", db)
                    return false
                }
                try WorkflowSQLite.execute(
                    "DELETE FROM jobs WHERE kind='scheduledAgentRun'",
                    db
                )
                try WorkflowSQLite.execute(
                    "DELETE FROM artifacts WHERE kind='scheduledOutput'",
                    db
                )
                try episodeStore.writeMetadata(metadata, in: db)
                try episodeStore.writeGeneration(nextGeneration, in: db)
                try WorkflowSQLite.execute("COMMIT TRANSACTION", db)
                return true
            } catch {
                try? WorkflowSQLite.execute("ROLLBACK TRANSACTION", db)
                throw error
            }
        }
        guard retired else { return false }
        revision.withLock { $0 = max($0, nextGeneration) }
        lastWrittenRevision.withLock { $0 = max($0, nextGeneration) }
        NotificationCenter.default.post(
            name: .persistenceDidCommitWorkflowJobs,
            object: self
        )
        return true
    }

    func legacyScheduledAgentSourceIsRetired(state: AppState) throws -> Bool {
        guard state.agentScheduledTasks.isEmpty else { return false }
        return try JobStore(
            fileURL: episodeStore.fileURL
        ).legacyScheduledAgentSourceIsRetired()
    }

    private static let scheduledAgentMetadataEncoder: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        return encoder
    }()
}

private extension UInt64 {
    var saturatingIncremented: UInt64 {
        self == .max ? .max : self + 1
    }
}
