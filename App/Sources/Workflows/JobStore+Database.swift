import CSQLiteVec
import Foundation

extension JobStore {
    static let columns = """
        id,idempotency_key,kind,subject_id,input_version,occurrence_id,
        payload_version,payload,state,priority,resource_class,attempt,max_attempts,
        not_before,lease_token,lease_owner,lease_expires_at,external_provider,
        external_operation_id,external_operation_state,output_version,last_error_class,last_error_message,
        created_at,updated_at
        """

    func withDatabase<T>(
        publishChanges: Bool = true,
        _ body: (OpaquePointer) throws -> T
    ) throws -> T {
        var changed = false
        let result = try WorkflowSQLite.withDatabase(fileURL: fileURL) { db in
            try ensureSchema(db)
            let before = sqlite3_total_changes(db)
            let value = try body(db)
            changed = sqlite3_total_changes(db) != before
            return value
        }
        if publishChanges, changed { WorkflowJobChangeSignal.post(fileURL: fileURL) }
        return result
    }

    func ensureSchema(_ db: OpaquePointer) throws {
        try WorkflowSchemaMigrations.ensureJobs(db)
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
