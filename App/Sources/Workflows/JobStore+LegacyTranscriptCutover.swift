import CSQLite3
import Foundation

extension JobStore {
    /// Quarantined source reader for the one-shot Rust transcript cutover.
    func legacyTranscriptJobs() throws -> [LegacyTranscriptWorkflowJob] {
        try withDatabase(publishChanges: false) { db in
            try legacyTranscriptJobs(db: db)
        }
    }

    /// Deletes only the exact source proven by the immutable rollback backup.
    func removeLegacyTranscriptJobs(
        matching expected: [LegacyTranscriptWorkflowJob]
    ) throws -> Bool {
        try withDatabase { db in
            try WorkflowSQLite.execute("BEGIN IMMEDIATE TRANSACTION", db)
            do {
                let current = try legacyTranscriptJobs(db: db)
                guard current == expected.sorted(by: Self.sortLegacyTranscriptJobs) else {
                    try WorkflowSQLite.execute("COMMIT TRANSACTION", db)
                    return false
                }
                let statement = try WorkflowSQLite.prepare(
                    "DELETE FROM jobs WHERE kind IN ('transcriptIngest','transcriptIndex')",
                    db: db
                )
                defer { sqlite3_finalize(statement) }
                try WorkflowSQLite.stepDone(statement, db)
                guard Int(sqlite3_changes(db)) == current.count else {
                    throw JobStoreError.transitionRejected
                }
                try WorkflowSQLite.execute("COMMIT TRANSACTION", db)
                return true
            } catch {
                try? WorkflowSQLite.execute("ROLLBACK TRANSACTION", db)
                throw error
            }
        }
    }

    func legacyTranscriptJobsAreRetired() throws -> Bool {
        try legacyTranscriptJobs().isEmpty
    }
}

private extension JobStore {
    static func sortLegacyTranscriptJobs(
        _ lhs: LegacyTranscriptWorkflowJob,
        _ rhs: LegacyTranscriptWorkflowJob
    ) -> Bool {
        lhs.id.uuidString < rhs.id.uuidString
    }

    func legacyTranscriptJobs(
        db: OpaquePointer
    ) throws -> [LegacyTranscriptWorkflowJob] {
        let statement = try WorkflowSQLite.prepare(
            "SELECT \(Self.columns) FROM jobs "
                + "WHERE kind IN ('transcriptIngest','transcriptIndex') ORDER BY id",
            db: db
        )
        defer { sqlite3_finalize(statement) }
        var jobs: [LegacyTranscriptWorkflowJob] = []
        while sqlite3_step(statement) == SQLITE_ROW {
            guard let id = WorkflowSQLite.text(statement, 0).flatMap(UUID.init(uuidString:)),
                  let key = WorkflowSQLite.text(statement, 1),
                  let kind = WorkflowSQLite.text(statement, 2)
                    .flatMap(LegacyTranscriptWorkflowJobKind.init(rawValue:)),
                  let subject = WorkflowSQLite.text(statement, 3).flatMap(UUID.init(uuidString:)),
                  let input = WorkflowSQLite.text(statement, 4),
                  let state = WorkflowSQLite.text(statement, 8).flatMap(WorkJobState.init(rawValue:)),
                  let resource = WorkflowSQLite.text(statement, 10)
                    .flatMap(WorkResourceClass.init(rawValue:)),
                  let notBefore = WorkflowSQLite.date(statement, 13),
                  let createdAt = WorkflowSQLite.date(statement, 23),
                  let updatedAt = WorkflowSQLite.date(statement, 24)
            else { throw JobStoreError.corruptRow }
            jobs.append(LegacyTranscriptWorkflowJob(
                id: id, idempotencyKey: key, kind: kind, subjectID: subject,
                inputVersion: input, occurrenceID: WorkflowSQLite.text(statement, 5),
                payloadVersion: Int(sqlite3_column_int64(statement, 6)),
                payload: WorkflowSQLite.data(statement, 7), state: state,
                priority: Int(sqlite3_column_int64(statement, 9)), resourceClass: resource,
                attempt: Int(sqlite3_column_int64(statement, 11)),
                maxAttempts: Int(sqlite3_column_int64(statement, 12)),
                notBefore: notBefore,
                leaseToken: WorkflowSQLite.text(statement, 14).flatMap(UUID.init(uuidString:)),
                leaseOwner: WorkflowSQLite.text(statement, 15),
                leaseExpiresAt: WorkflowSQLite.date(statement, 16),
                externalProvider: WorkflowSQLite.text(statement, 17),
                externalOperationID: WorkflowSQLite.text(statement, 18),
                externalOperationState: WorkflowSQLite.text(statement, 19),
                outputVersion: WorkflowSQLite.text(statement, 20),
                lastErrorClass: WorkflowSQLite.text(statement, 21)
                    .flatMap(JobErrorClass.init(rawValue:)),
                lastErrorMessage: WorkflowSQLite.text(statement, 22),
                createdAt: createdAt, updatedAt: updatedAt
            ))
        }
        return jobs.sorted(by: Self.sortLegacyTranscriptJobs)
    }
}
