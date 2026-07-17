import CSQLiteVec
import Foundation

struct JobStore: Sendable {
    let fileURL: URL

    @discardableResult
    func ensureJobs(_ desired: [DesiredJob]) throws -> Int {
        try withDatabase { db in
            let before = sqlite3_total_changes(db)
            try Self.ensureJobs(desired, in: db)
            return Int(sqlite3_total_changes(db) - before)
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
                    WHERE resource_class=? AND state IN ('leased','running')
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
                    WHERE resource_class = ?
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
        try withDatabase { db in
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

extension JobStore {
    static let columns = """
        id,idempotency_key,kind,subject_id,input_version,occurrence_id,
        payload_version,payload,state,priority,resource_class,attempt,max_attempts,
        not_before,lease_token,lease_owner,lease_expires_at,external_provider,
        external_operation_id,external_operation_state,output_version,last_error_class,last_error_message,
        created_at,updated_at
        """

    func withDatabase<T>(_ body: (OpaquePointer) throws -> T) throws -> T {
        try WorkflowSQLite.withDatabase(fileURL: fileURL) { db in
            try ensureSchema(db)
            return try body(db)
        }
    }

    func ensureSchema(_ db: OpaquePointer) throws {
        if try WorkflowSQLite.tableExists("jobs", db),
           try !WorkflowSQLite.columnExists("idempotency_key", table: "jobs", db) {
            try WorkflowSQLite.execute("DROP TABLE jobs", db)
        }
        try WorkflowSQLite.execute(
            """
            CREATE TABLE IF NOT EXISTS jobs(
                id TEXT PRIMARY KEY NOT NULL,
                idempotency_key TEXT UNIQUE NOT NULL,
                kind TEXT NOT NULL, subject_id TEXT NOT NULL,
                input_version TEXT NOT NULL, occurrence_id TEXT,
                payload_version INTEGER NOT NULL, payload BLOB,
                state TEXT NOT NULL, priority INTEGER NOT NULL,
                resource_class TEXT NOT NULL, attempt INTEGER NOT NULL,
                max_attempts INTEGER NOT NULL, not_before REAL NOT NULL,
                lease_token TEXT, lease_owner TEXT, lease_expires_at REAL,
                external_provider TEXT, external_operation_id TEXT,
                external_operation_state TEXT, output_version TEXT,
                last_error_class TEXT, last_error_message TEXT,
                created_at REAL NOT NULL, updated_at REAL NOT NULL
            )
            """,
            db
        )
        try WorkflowSQLite.execute(
            "CREATE INDEX IF NOT EXISTS jobs_due_v2 ON jobs(resource_class,state,not_before,priority)",
            db
        )
    }

    func reclaimExpiredLeases(
        now: Date,
        excludingOwner: String?,
        db: OpaquePointer
    ) throws {
        let statement = try WorkflowSQLite.prepare(
            """
            UPDATE jobs SET state=CASE
                    WHEN resource_class='remoteSTT' AND external_operation_id IS NULL
                    THEN 'blocked'
                    WHEN attempt >= max_attempts THEN 'failedPermanent'
                    ELSE 'retryScheduled' END,
                not_before=?, lease_token=NULL,
                lease_owner=NULL, lease_expires_at=NULL,
                last_error_class=CASE
                    WHEN resource_class='remoteSTT' AND external_operation_id IS NULL
                    THEN 'unsafeToRetry' ELSE 'transient' END,
                last_error_message=CASE
                    WHEN resource_class='remoteSTT' AND external_operation_id IS NULL
                    THEN 'Interrupted remote submission has no resumable provider ID; manual retry required'
                    WHEN attempt >= max_attempts
                    THEN 'Attempt limit reached while recovering an interrupted lease'
                    ELSE 'Lease expired after interruption' END,
                updated_at=?
            WHERE state IN ('leased','running') AND lease_expires_at <= ?
              AND (? IS NULL OR lease_owner IS NULL OR lease_owner <> ?)
            """,
            db: db
        )
        defer { sqlite3_finalize(statement) }
        try WorkflowSQLite.bind(now, 1, statement, db)
        try WorkflowSQLite.bind(now, 2, statement, db)
        try WorkflowSQLite.bind(now, 3, statement, db)
        try WorkflowSQLite.bind(excludingOwner, 4, statement, db)
        try WorkflowSQLite.bind(excludingOwner, 5, statement, db)
        try WorkflowSQLite.stepDone(statement, db)
    }

    func failExhaustedJobs(now: Date, db: OpaquePointer) throws {
        let statement = try WorkflowSQLite.prepare(
            """
            UPDATE jobs SET state='failedPermanent', lease_token=NULL,
                lease_owner=NULL, lease_expires_at=NULL,
                last_error_class=COALESCE(last_error_class,'unexpected'),
                last_error_message=COALESCE(
                    last_error_message,
                    'Attempt limit reached before another claim'
                ),
                updated_at=?
            WHERE state IN ('pending','retryScheduled')
              AND attempt >= max_attempts
            """,
            db: db
        )
        defer { sqlite3_finalize(statement) }
        try WorkflowSQLite.bind(now, 1, statement, db)
        try WorkflowSQLite.stepDone(statement, db)
    }

    func transition(_ sql: String, id: UUID, leaseToken: UUID, now: Date) throws {
        try withDatabase { db in
            let statement = try WorkflowSQLite.prepare(sql, db: db)
            defer { sqlite3_finalize(statement) }
            try WorkflowSQLite.bind(now, 1, statement, db)
            try WorkflowSQLite.bind(id.uuidString, 2, statement, db)
            try WorkflowSQLite.bind(leaseToken.uuidString, 3, statement, db)
            try WorkflowSQLite.stepDone(statement, db)
            guard sqlite3_changes(db) == 1 else { throw JobStoreError.transitionRejected }
        }
    }

    func finish(
        id: UUID,
        leaseToken: UUID,
        state: WorkJobState,
        notBefore: Date? = nil,
        failure: JobFailure? = nil,
        outputVersion: String? = nil,
        now: Date = Date()
    ) throws {
        try withDatabase { db in
            let statement = try WorkflowSQLite.prepare(
                """
                UPDATE jobs SET state=?, not_before=COALESCE(?,not_before),
                    output_version=COALESCE(?,output_version), last_error_class=?,
                    last_error_message=?, lease_token=NULL, lease_owner=NULL,
                    lease_expires_at=NULL, updated_at=?
                WHERE id=? AND state IN ('leased','running') AND lease_token=?
                """,
                db: db
            )
            defer { sqlite3_finalize(statement) }
            try WorkflowSQLite.bind(state.rawValue, 1, statement, db)
            try WorkflowSQLite.bind(notBefore, 2, statement, db)
            try WorkflowSQLite.bind(outputVersion, 3, statement, db)
            try WorkflowSQLite.bind(failure?.classification.rawValue, 4, statement, db)
            try WorkflowSQLite.bind(failure?.message, 5, statement, db)
            try WorkflowSQLite.bind(now, 6, statement, db)
            try WorkflowSQLite.bind(id.uuidString, 7, statement, db)
            try WorkflowSQLite.bind(leaseToken.uuidString, 8, statement, db)
            try WorkflowSQLite.stepDone(statement, db)
            guard sqlite3_changes(db) == 1 else { throw JobStoreError.transitionRejected }
        }
    }

    func updateActiveTerminal(id: UUID, state: WorkJobState) throws {
        try withDatabase { db in
            let statement = try WorkflowSQLite.prepare(
                """
                UPDATE jobs SET state=?,lease_token=NULL,lease_owner=NULL,
                    lease_expires_at=NULL,updated_at=?
                WHERE id=? AND state IN ('pending','leased','running','retryScheduled','blocked')
                """,
                db: db
            )
            defer { sqlite3_finalize(statement) }
            try WorkflowSQLite.bind(state.rawValue, 1, statement, db)
            try WorkflowSQLite.bind(Date(), 2, statement, db)
            try WorkflowSQLite.bind(id.uuidString, 3, statement, db)
            try WorkflowSQLite.stepDone(statement, db)
        }
    }

    func load(id: UUID, db: OpaquePointer) throws -> WorkJob? {
        let statement = try WorkflowSQLite.prepare("SELECT \(Self.columns) FROM jobs WHERE id=?", db: db)
        defer { sqlite3_finalize(statement) }
        try WorkflowSQLite.bind(id.uuidString, 1, statement, db)
        return try readRows(statement).first
    }

    func readRows(_ statement: OpaquePointer) throws -> [WorkJob] {
        var jobs: [WorkJob] = []
        while sqlite3_step(statement) == SQLITE_ROW {
            guard let id = WorkflowSQLite.text(statement, 0).flatMap(UUID.init(uuidString:)),
                  let key = WorkflowSQLite.text(statement, 1),
                  let kind = WorkflowSQLite.text(statement, 2).flatMap(WorkJobKind.init(rawValue:)),
                  let subject = WorkflowSQLite.text(statement, 3).flatMap(UUID.init(uuidString:)),
                  let input = WorkflowSQLite.text(statement, 4),
                  let state = WorkflowSQLite.text(statement, 8).flatMap(WorkJobState.init(rawValue:)),
                  let resource = WorkflowSQLite.text(statement, 10).flatMap(WorkResourceClass.init(rawValue:)) else {
                throw JobStoreError.corruptRow
            }
            jobs.append(WorkJob(
                id: id, idempotencyKey: key, kind: kind, subjectID: subject,
                inputVersion: input, occurrenceID: WorkflowSQLite.text(statement, 5),
                payloadVersion: Int(sqlite3_column_int64(statement, 6)),
                payload: WorkflowSQLite.data(statement, 7), state: state,
                priority: Int(sqlite3_column_int64(statement, 9)), resourceClass: resource,
                attempt: Int(sqlite3_column_int64(statement, 11)),
                maxAttempts: Int(sqlite3_column_int64(statement, 12)),
                notBefore: WorkflowSQLite.date(statement, 13)!,
                leaseToken: WorkflowSQLite.text(statement, 14).flatMap(UUID.init(uuidString:)),
                leaseOwner: WorkflowSQLite.text(statement, 15),
                leaseExpiresAt: WorkflowSQLite.date(statement, 16),
                externalProvider: WorkflowSQLite.text(statement, 17),
                externalOperationID: WorkflowSQLite.text(statement, 18),
                externalOperationState: WorkflowSQLite.text(statement, 19),
                outputVersion: WorkflowSQLite.text(statement, 20),
                lastErrorClass: WorkflowSQLite.text(statement, 21).flatMap(JobErrorClass.init(rawValue:)),
                lastErrorMessage: WorkflowSQLite.text(statement, 22),
                createdAt: WorkflowSQLite.date(statement, 23)!,
                updatedAt: WorkflowSQLite.date(statement, 24)!
            ))
        }
        return jobs
    }
}
