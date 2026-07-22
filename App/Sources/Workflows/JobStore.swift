import CSQLite3
import Foundation

struct JobStore: Sendable {
    let fileURL: URL

    @discardableResult
    func ensureJobs(
        _ desired: [DesiredJob],
        afterEach: (Int) throws -> Void = { _ in }
    ) throws -> Int {
        try withDatabase { db in
            try WorkflowSQLite.execute("BEGIN IMMEDIATE TRANSACTION", db)
            do {
                let before = sqlite3_total_changes(db)
                try Self.ensureJobs(desired, in: db, afterEach: afterEach)
                let inserted = Int(sqlite3_total_changes(db) - before)
                try WorkflowSQLite.execute("COMMIT TRANSACTION", db)
                return inserted
            } catch {
                try? WorkflowSQLite.execute("ROLLBACK TRANSACTION", db)
                throw error
            }
        }
    }

    @discardableResult
    func ensureJob(_ desired: DesiredJob, notBefore: Date = Date()) throws -> Bool {
        try withDatabase { db in
            let statement = try WorkflowSQLite.prepare(
                """
                INSERT INTO jobs(
                    id, idempotency_key, kind, subject_id, input_version,
                    occurrence_id, payload_version, payload, state, priority,
                    resource_class, attempt, max_attempts, not_before,
                    created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?, ?, 0, ?, ?, ?, ?)
                ON CONFLICT(idempotency_key) DO NOTHING
                """,
                db: db
            )
            defer { sqlite3_finalize(statement) }
            let now = Date()
            try WorkflowSQLite.bind(UUID().uuidString, 1, statement, db)
            try WorkflowSQLite.bind(desired.idempotencyKey, 2, statement, db)
            try WorkflowSQLite.bind(desired.kind.rawValue, 3, statement, db)
            try WorkflowSQLite.bind(desired.subjectID.uuidString, 4, statement, db)
            try WorkflowSQLite.bind(desired.inputVersion, 5, statement, db)
            try WorkflowSQLite.bind(desired.occurrenceID, 6, statement, db)
            try WorkflowSQLite.bind(Int64(desired.payloadVersion), 7, statement, db)
            try WorkflowSQLite.bind(desired.payload, 8, statement, db)
            try WorkflowSQLite.bind(Int64(desired.priority), 9, statement, db)
            try WorkflowSQLite.bind(desired.resourceClass.rawValue, 10, statement, db)
            try WorkflowSQLite.bind(Int64(desired.maxAttempts), 11, statement, db)
            try WorkflowSQLite.bind(notBefore, 12, statement, db)
            try WorkflowSQLite.bind(now, 13, statement, db)
            try WorkflowSQLite.bind(now, 14, statement, db)
            try WorkflowSQLite.stepDone(statement, db)
            return sqlite3_changes(db) == 1
        }
    }

    func claimDueJobs(
        resourceClass: WorkResourceClass,
        capacity: Int,
        now: Date,
        owner: String,
        leaseDuration: TimeInterval
    ) throws -> [WorkJob] {
        guard capacity > 0 else { return [] }
        return try withDatabase { db in
            try WorkflowSQLite.execute("BEGIN IMMEDIATE TRANSACTION", db)
            do {
                try reclaimExpiredLeases(now: now, excludingOwner: nil, db: db)
                try failExhaustedJobs(now: now, db: db)
                let count = try WorkflowSQLite.prepare(
                    """
                    SELECT COUNT(*) FROM jobs
                    WHERE kind IN (\(Self.supportedKindSQL))
                      AND resource_class=? AND state IN ('leased','running')
                    """,
                    db: db
                )
                try WorkflowSQLite.bind(resourceClass.rawValue, 1, count, db)
                let occupied = sqlite3_step(count) == SQLITE_ROW
                    ? Int(sqlite3_column_int64(count, 0)) : capacity
                sqlite3_finalize(count)
                let slots = max(0, capacity - occupied)
                guard slots > 0 else {
                    try WorkflowSQLite.execute("COMMIT TRANSACTION", db)
                    return []
                }
                let select = try WorkflowSQLite.prepare(
                    """
                    SELECT id FROM jobs
                    WHERE kind IN (\(Self.supportedKindSQL))
                      AND resource_class = ?
                      AND state IN ('pending','retryScheduled')
                      AND not_before <= ?
                      AND attempt < max_attempts
                    ORDER BY priority DESC, not_before ASC, created_at ASC, id ASC
                    LIMIT ?
                    """,
                    db: db
                )
                defer { sqlite3_finalize(select) }
                try WorkflowSQLite.bind(resourceClass.rawValue, 1, select, db)
                try WorkflowSQLite.bind(now, 2, select, db)
                try WorkflowSQLite.bind(Int64(slots), 3, select, db)
                var ids: [UUID] = []
                while sqlite3_step(select) == SQLITE_ROW {
                    if let value = WorkflowSQLite.text(select, 0).flatMap(UUID.init(uuidString:)) {
                        ids.append(value)
                    }
                }

                var claimed: [WorkJob] = []
                for id in ids {
                    let token = UUID()
                    let update = try WorkflowSQLite.prepare(
                        """
                        UPDATE jobs SET state='leased', attempt=attempt+1,
                            lease_token=?, lease_owner=?, lease_expires_at=?, updated_at=?
                        WHERE id=? AND state IN ('pending','retryScheduled')
                        """,
                        db: db
                    )
                    try WorkflowSQLite.bind(token.uuidString, 1, update, db)
                    try WorkflowSQLite.bind(owner, 2, update, db)
                    try WorkflowSQLite.bind(now.addingTimeInterval(leaseDuration), 3, update, db)
                    try WorkflowSQLite.bind(now, 4, update, db)
                    try WorkflowSQLite.bind(id.uuidString, 5, update, db)
                    try WorkflowSQLite.stepDone(update, db)
                    sqlite3_finalize(update)
                    if sqlite3_changes(db) == 1,
                       let job = try load(id: id, db: db) {
                        claimed.append(job)
                    }
                }
                try WorkflowSQLite.execute("COMMIT TRANSACTION", db)
                return claimed
            } catch {
                try? WorkflowSQLite.execute("ROLLBACK TRANSACTION", db)
                throw error
            }
        }
    }

    func markRunning(id: UUID, leaseToken: UUID, now: Date = Date()) throws {
        try transition(
            "UPDATE jobs SET state='running', updated_at=? WHERE id=? AND state='leased' AND lease_token=?",
            id: id,
            leaseToken: leaseToken,
            now: now
        )
    }

    func renewLease(
        id: UUID,
        leaseToken: UUID,
        expiresAt: Date,
        now: Date = Date()
    ) throws {
        try withDatabase(publishChanges: false) { db in
            let statement = try WorkflowSQLite.prepare(
                """
                UPDATE jobs SET lease_expires_at=?, updated_at=?
                WHERE id=? AND state IN ('leased','running') AND lease_token=?
                """,
                db: db
            )
            defer { sqlite3_finalize(statement) }
            try WorkflowSQLite.bind(expiresAt, 1, statement, db)
            try WorkflowSQLite.bind(now, 2, statement, db)
            try WorkflowSQLite.bind(id.uuidString, 3, statement, db)
            try WorkflowSQLite.bind(leaseToken.uuidString, 4, statement, db)
            try WorkflowSQLite.stepDone(statement, db)
            guard sqlite3_changes(db) == 1 else { throw JobStoreError.transitionRejected }
        }
    }

    func recordExternalOperation(
        id: UUID,
        leaseToken: UUID,
        provider: String,
        externalID: String,
        state: String,
        now: Date = Date()
    ) throws {
        try withDatabase { db in
            let statement = try WorkflowSQLite.prepare(
                """
                UPDATE jobs SET external_provider=?, external_operation_id=?,
                    external_operation_state=?, updated_at=?
                WHERE id=? AND state IN ('leased','running') AND lease_token=?
                """,
                db: db
            )
            defer { sqlite3_finalize(statement) }
            try WorkflowSQLite.bind(provider, 1, statement, db)
            try WorkflowSQLite.bind(externalID, 2, statement, db)
            try WorkflowSQLite.bind(state, 3, statement, db)
            try WorkflowSQLite.bind(now, 4, statement, db)
            try WorkflowSQLite.bind(id.uuidString, 5, statement, db)
            try WorkflowSQLite.bind(leaseToken.uuidString, 6, statement, db)
            try WorkflowSQLite.stepDone(statement, db)
            guard sqlite3_changes(db) == 1 else { throw JobStoreError.transitionRejected }
        }
    }

    func complete(
        id: UUID,
        leaseToken: UUID,
        outputVersion: String?,
        now: Date = Date()
    ) throws {
        try finish(
            id: id,
            leaseToken: leaseToken,
            state: .succeeded,
            notBefore: nil,
            failure: nil,
            outputVersion: outputVersion,
            now: now
        )
    }

    func scheduleRetry(
        id: UUID,
        leaseToken: UUID,
        notBefore: Date,
        error: JobFailure,
        now: Date = Date()
    ) throws {
        try finish(
            id: id,
            leaseToken: leaseToken,
            state: .retryScheduled,
            notBefore: notBefore,
            failure: error,
            outputVersion: nil,
            now: now
        )
    }

    func markBlocked(id: UUID, leaseToken: UUID, reason: JobFailure) throws {
        try finish(id: id, leaseToken: leaseToken, state: .blocked, failure: reason)
    }

    func markFailedPermanent(id: UUID, leaseToken: UUID, error: JobFailure) throws {
        try finish(id: id, leaseToken: leaseToken, state: .failedPermanent, failure: error)
    }

    func markObsolete(id: UUID, leaseToken: UUID) throws {
        try finish(id: id, leaseToken: leaseToken, state: .obsolete)
    }

    func markCancelled(id: UUID, leaseToken: UUID) throws {
        try finish(id: id, leaseToken: leaseToken, state: .cancelled)
    }

}
