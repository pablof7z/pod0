import CSQLiteVec
import Foundation

extension JobStore {
    func unblockAll(now: Date = Date()) throws {
        try withDatabase { db in
            let statement = try WorkflowSQLite.prepare(
                """
                UPDATE jobs SET state='pending', not_before=?, updated_at=?
                WHERE state='blocked'
                  AND last_error_class IN ('missingCredential','missingDependency')
                """,
                db: db
            )
            defer { sqlite3_finalize(statement) }
            try WorkflowSQLite.bind(now, 1, statement, db)
            try WorkflowSQLite.bind(now, 2, statement, db)
            try WorkflowSQLite.stepDone(statement, db)
        }
    }

    func unblock(idempotencyKey: String, now: Date = Date()) throws {
        try withDatabase { db in
            let statement = try WorkflowSQLite.prepare(
                """
                UPDATE jobs SET state='pending', not_before=?, updated_at=?,
                    last_error_class=NULL,last_error_message=NULL
                WHERE idempotency_key=? AND state='blocked'
                  AND last_error_class IN ('missingCredential','missingDependency')
                """,
                db: db
            )
            defer { sqlite3_finalize(statement) }
            try WorkflowSQLite.bind(now, 1, statement, db)
            try WorkflowSQLite.bind(now, 2, statement, db)
            try WorkflowSQLite.bind(idempotencyKey, 3, statement, db)
            try WorkflowSQLite.stepDone(statement, db)
        }
    }

    func makeDependencyRetriesDue(kind: WorkJobKind, now: Date = Date()) throws {
        try withDatabase { db in
            let statement = try WorkflowSQLite.prepare(
                """
                UPDATE jobs SET not_before=?,updated_at=?
                WHERE kind=? AND state='retryScheduled'
                  AND last_error_class='missingDependency'
                """,
                db: db
            )
            defer { sqlite3_finalize(statement) }
            try WorkflowSQLite.bind(now, 1, statement, db)
            try WorkflowSQLite.bind(now, 2, statement, db)
            try WorkflowSQLite.bind(kind.rawValue, 3, statement, db)
            try WorkflowSQLite.stepDone(statement, db)
        }
    }

    func manuallyRetry(kind: WorkJobKind, subjectID: UUID, now: Date = Date()) throws {
        try withDatabase { db in
            let statement = try WorkflowSQLite.prepare(
                """
                UPDATE jobs SET state='pending', attempt=0, not_before=?,
                    lease_token=NULL, lease_owner=NULL, lease_expires_at=NULL,
                    external_provider=NULL, external_operation_id=NULL,
                    external_operation_state=NULL, last_error_class=NULL,
                    last_error_message=NULL, updated_at=?
                WHERE kind=? AND subject_id=? AND state IN ('blocked','failedPermanent')
                """,
                db: db
            )
            defer { sqlite3_finalize(statement) }
            try WorkflowSQLite.bind(now, 1, statement, db)
            try WorkflowSQLite.bind(now, 2, statement, db)
            try WorkflowSQLite.bind(kind.rawValue, 3, statement, db)
            try WorkflowSQLite.bind(subjectID.uuidString, 4, statement, db)
            try WorkflowSQLite.stepDone(statement, db)
        }
    }

    /// Re-arms one canonical occurrence after an explicit user retry. Keeping
    /// the same row preserves its durable identity while clearing ownership
    /// and provider state from the prior terminal attempt.
    func rearmJob(idempotencyKey: String, now: Date = Date()) throws {
        try withDatabase { db in
            let statement = try WorkflowSQLite.prepare(
                """
                UPDATE jobs SET state='pending', attempt=0, not_before=?,
                    lease_token=NULL, lease_owner=NULL, lease_expires_at=NULL,
                    external_provider=NULL, external_operation_id=NULL,
                    external_operation_state=NULL, last_error_class=NULL,
                    last_error_message=NULL, updated_at=?
                WHERE idempotency_key=?
                  AND state IN ('blocked','failedPermanent','cancelled')
                """,
                db: db
            )
            defer { sqlite3_finalize(statement) }
            try WorkflowSQLite.bind(now, 1, statement, db)
            try WorkflowSQLite.bind(now, 2, statement, db)
            try WorkflowSQLite.bind(idempotencyKey, 3, statement, db)
            try WorkflowSQLite.stepDone(statement, db)
        }
    }

    /// Re-arms derivable work whose previously selected output no longer
    /// satisfies desired state. The canonical row remains the lineage anchor:
    /// attempts are retained, a fresh attempt budget is added, and any old
    /// lease or provider identity is fenced before the repair becomes runnable.
    @discardableResult
    func rearmSucceededRepairs(
        _ desired: [DesiredJob],
        now: Date = Date()
    ) throws -> Int {
        try withDatabase { db in
            let statement = try WorkflowSQLite.prepare(
                """
                UPDATE jobs SET state='pending', max_attempts=max_attempts+?,
                    not_before=?, lease_token=NULL, lease_owner=NULL,
                    lease_expires_at=NULL, external_provider=NULL,
                    external_operation_id=NULL, external_operation_state=NULL,
                    last_error_class=NULL, last_error_message=NULL, updated_at=?
                WHERE idempotency_key=? AND kind=? AND subject_id=?
                  AND input_version=? AND occurrence_id IS NULL
                  AND state='succeeded'
                """,
                db: db
            )
            defer { sqlite3_finalize(statement) }
            let before = sqlite3_total_changes(db)
            for job in desired where job.occurrenceID == nil {
                try WorkflowSQLite.bind(Int64(job.maxAttempts), 1, statement, db)
                try WorkflowSQLite.bind(now, 2, statement, db)
                try WorkflowSQLite.bind(now, 3, statement, db)
                try WorkflowSQLite.bind(job.idempotencyKey, 4, statement, db)
                try WorkflowSQLite.bind(job.kind.rawValue, 5, statement, db)
                try WorkflowSQLite.bind(job.subjectID.uuidString, 6, statement, db)
                try WorkflowSQLite.bind(job.inputVersion, 7, statement, db)
                try WorkflowSQLite.stepDone(statement, db)
                sqlite3_reset(statement)
                sqlite3_clear_bindings(statement)
            }
            return Int(sqlite3_total_changes(db) - before)
        }
    }

    func obsoleteActiveJobs(notIn keys: Set<String>, derivableOnly: Bool = true) throws {
        let jobs = try allJobs().filter { $0.state.isActive }
        for job in jobs where !keys.contains(job.idempotencyKey) {
            if derivableOnly, job.occurrenceID != nil { continue }
            try updateActiveTerminal(id: job.id, state: .obsolete)
        }
    }

    func allJobs() throws -> [WorkJob] {
        try withDatabase { db in
            let statement = try WorkflowSQLite.prepare(
                "SELECT \(Self.columns) FROM jobs ORDER BY created_at, id", db: db
            )
            defer { sqlite3_finalize(statement) }
            return try readRows(statement)
        }
    }

    func job(idempotencyKey: String) throws -> WorkJob? {
        try withDatabase { db in
            let statement = try WorkflowSQLite.prepare(
                "SELECT \(Self.columns) FROM jobs WHERE idempotency_key=?", db: db
            )
            defer { sqlite3_finalize(statement) }
            try WorkflowSQLite.bind(idempotencyKey, 1, statement, db)
            return try readRows(statement).first
        }
    }

    func job(id: UUID) throws -> WorkJob? {
        try withDatabase { db in try load(id: id, db: db) }
    }

    func nextDueDate() throws -> Date? {
        try withDatabase { db in
            let statement = try WorkflowSQLite.prepare(
                "SELECT MIN(not_before) FROM jobs WHERE state IN ('pending','retryScheduled')",
                db: db
            )
            defer { sqlite3_finalize(statement) }
            guard sqlite3_step(statement) == SQLITE_ROW,
                  sqlite3_column_type(statement, 0) != SQLITE_NULL else { return nil }
            return Date(timeIntervalSince1970: sqlite3_column_double(statement, 0))
        }
    }

    func reclaimExpiredLeases(now: Date = Date()) throws {
        try withDatabase { db in
            try reclaimExpiredLeases(now: now, excludingOwner: nil, db: db)
        }
    }

    func requeueInterrupted(id: UUID, now: Date = Date()) throws {
        try withDatabase { db in
            let statement = try WorkflowSQLite.prepare(
                """
                UPDATE jobs SET state=CASE WHEN attempt >= max_attempts
                        THEN 'failedPermanent' ELSE 'retryScheduled' END,
                    not_before=?,
                    lease_token=NULL, lease_owner=NULL, lease_expires_at=NULL,
                    last_error_class=CASE WHEN attempt >= max_attempts
                        THEN 'unexpected' ELSE 'transient' END,
                    last_error_message=CASE WHEN attempt >= max_attempts
                        THEN 'Attempt limit reached while recovering an interrupted transfer'
                        ELSE 'Transfer observer was reconstructed after interruption' END,
                    updated_at=? WHERE id=? AND state IN ('leased','running')
                """,
                db: db
            )
            defer { sqlite3_finalize(statement) }
            try WorkflowSQLite.bind(now, 1, statement, db)
            try WorkflowSQLite.bind(now, 2, statement, db)
            try WorkflowSQLite.bind(id.uuidString, 3, statement, db)
            try WorkflowSQLite.stepDone(statement, db)
        }
    }

    /// Fences attempts whose in-process task disappeared while their durable
    /// lease still names this coordinator. This is safe only when the caller
    /// has no matching active task.
    func requeueAbandoned(owner: String, now: Date = Date()) throws {
        try withDatabase { db in
            let statement = try WorkflowSQLite.prepare(
                """
                UPDATE jobs SET state=CASE
                        WHEN resource_class='remoteSTT' AND external_operation_id IS NULL
                        THEN 'blocked'
                        WHEN attempt >= max_attempts THEN 'failedPermanent'
                        ELSE 'retryScheduled' END,
                    not_before=?,lease_token=NULL,lease_owner=NULL,lease_expires_at=NULL,
                    last_error_class=CASE
                        WHEN resource_class='remoteSTT' AND external_operation_id IS NULL
                        THEN 'unsafeToRetry'
                        WHEN attempt >= max_attempts THEN 'unexpected'
                        ELSE 'transient' END,
                    last_error_message=CASE
                        WHEN attempt >= max_attempts
                        THEN 'Attempt limit reached after the coordinator task disappeared'
                        ELSE 'Coordinator task disappeared before a terminal commit' END,
                    updated_at=?
                WHERE lease_owner=? AND state IN ('leased','running')
                """,
                db: db
            )
            defer { sqlite3_finalize(statement) }
            try WorkflowSQLite.bind(now, 1, statement, db)
            try WorkflowSQLite.bind(now, 2, statement, db)
            try WorkflowSQLite.bind(owner, 3, statement, db)
            try WorkflowSQLite.stepDone(statement, db)
        }
    }

    func removeAll() throws {
        try withDatabase { db in try WorkflowSQLite.execute("DELETE FROM jobs", db) }
    }

    func removeDerivableJobs() throws {
        try withDatabase { db in
            try WorkflowSQLite.execute("DELETE FROM jobs WHERE occurrence_id IS NULL", db)
        }
    }

    func cancelActiveJobs(kind: WorkJobKind, subjectID: UUID, now: Date = Date()) throws {
        try withDatabase { db in
            let statement = try WorkflowSQLite.prepare(
                """
                UPDATE jobs SET state='cancelled',lease_token=NULL,lease_owner=NULL,
                    lease_expires_at=NULL,last_error_class='cancelled',
                    last_error_message='Cancelled by user',updated_at=?
                WHERE kind=? AND subject_id=?
                  AND state IN ('pending','leased','running','retryScheduled','blocked')
                """,
                db: db
            )
            defer { sqlite3_finalize(statement) }
            try WorkflowSQLite.bind(now, 1, statement, db)
            try WorkflowSQLite.bind(kind.rawValue, 2, statement, db)
            try WorkflowSQLite.bind(subjectID.uuidString, 3, statement, db)
            try WorkflowSQLite.stepDone(statement, db)
        }
    }

    /// Hides terminal work the user has explicitly dismissed. Unlike changing
    /// the episode's stable artifact projection, this updates the authoritative
    /// lifecycle row. A later explicit retry can re-arm the same canonical job.
    func dismissJobsNeedingAttention(
        kind: WorkJobKind,
        subjectID: UUID,
        now: Date = Date()
    ) throws {
        try withDatabase { db in
            let statement = try WorkflowSQLite.prepare(
                """
                UPDATE jobs SET state='cancelled',lease_token=NULL,lease_owner=NULL,
                    lease_expires_at=NULL,last_error_class='cancelled',
                    last_error_message='Dismissed by user',updated_at=?
                WHERE kind=? AND subject_id=?
                  AND state IN ('blocked','failedPermanent')
                """,
                db: db
            )
            defer { sqlite3_finalize(statement) }
            try WorkflowSQLite.bind(now, 1, statement, db)
            try WorkflowSQLite.bind(kind.rawValue, 2, statement, db)
            try WorkflowSQLite.bind(subjectID.uuidString, 3, statement, db)
            try WorkflowSQLite.stepDone(statement, db)
        }
    }
}
