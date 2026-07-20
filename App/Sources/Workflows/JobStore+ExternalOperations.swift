import CSQLite3
import Foundation

extension JobStore {
    /// Persists the unsafe submission boundary before a remote request leaves
    /// the process. A missing provider operation ID then means "do not retry"
    /// if the lease is interrupted before the response can be recorded.
    func recordExternalSubmissionIntent(
        id: UUID,
        leaseToken: UUID,
        provider: String,
        state: String = "submitting",
        now: Date = Date()
    ) throws {
        try withDatabase { db in
            let statement = try WorkflowSQLite.prepare(
                """
                UPDATE jobs SET external_provider=?, external_operation_id=NULL,
                    external_operation_state=?, updated_at=?
                WHERE id=? AND state IN ('leased','running') AND lease_token=?
                """,
                db: db
            )
            defer { sqlite3_finalize(statement) }
            try WorkflowSQLite.bind(provider, 1, statement, db)
            try WorkflowSQLite.bind(state, 2, statement, db)
            try WorkflowSQLite.bind(now, 3, statement, db)
            try WorkflowSQLite.bind(id.uuidString, 4, statement, db)
            try WorkflowSQLite.bind(leaseToken.uuidString, 5, statement, db)
            try WorkflowSQLite.stepDone(statement, db)
            guard sqlite3_changes(db) == 1 else {
                throw JobStoreError.transitionRejected
            }
        }
    }
}
