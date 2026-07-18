import CSQLiteVec
import Foundation

extension JobStore {
    func perform(
        _ action: WorkflowJobAction,
        jobID: UUID,
        expectedUpdatedAt: Date,
        now: Date = Date()
    ) throws -> WorkflowJobActionResult {
        try withDatabase { db in
            guard let job = try load(id: jobID, db: db) else { return .notFound }
            guard abs(job.updatedAt.timeIntervalSince(expectedUpdatedAt)) < 0.000_001 else {
                return .stale
            }
            let projection = WorkflowJobProjection(job: job)
            guard projection.allowedActions.contains(action) else {
                switch job.state {
                case .succeeded, .obsolete, .cancelled: return .alreadyComplete
                default: return .notAllowed
                }
            }

            let statement = try WorkflowSQLite.prepare(sql(for: action), db: db)
            defer { sqlite3_finalize(statement) }
            switch action {
            case .retry:
                try WorkflowSQLite.bind(now, 1, statement, db)
                try WorkflowSQLite.bind(now, 2, statement, db)
                try WorkflowSQLite.bind(jobID.uuidString, 3, statement, db)
                try WorkflowSQLite.bind(expectedUpdatedAt, 4, statement, db)
            case .cancel:
                try WorkflowSQLite.bind(now, 1, statement, db)
                try WorkflowSQLite.bind(jobID.uuidString, 2, statement, db)
                try WorkflowSQLite.bind(expectedUpdatedAt, 3, statement, db)
            }
            try WorkflowSQLite.stepDone(statement, db)
            guard sqlite3_changes(db) == 1 else { return .stale }
            return .accepted(action)
        }
    }

    private func sql(for action: WorkflowJobAction) -> String {
        switch action {
        case .retry:
            return """
                UPDATE jobs SET state='pending', attempt=0, not_before=?,
                    lease_token=NULL, lease_owner=NULL, lease_expires_at=NULL,
                    external_provider=NULL, external_operation_id=NULL,
                    external_operation_state=NULL, last_error_class=NULL,
                    last_error_message=NULL, updated_at=?
                WHERE id=? AND updated_at=?
                  AND state IN ('blocked','failedPermanent','cancelled')
                  AND COALESCE(last_error_class,'') NOT IN
                      ('unsafeToRetry','invalidInput','unsupportedFormat')
                """
        case .cancel:
            return """
                UPDATE jobs SET state='cancelled',lease_token=NULL,lease_owner=NULL,
                    lease_expires_at=NULL,last_error_class='cancelled',
                    last_error_message='Cancelled by user',updated_at=?
                WHERE id=? AND updated_at=?
                  AND state IN ('pending','leased','running','retryScheduled','blocked')
                """
        }
    }
}
