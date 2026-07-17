import CSQLiteVec
import Foundation

extension JobStore {
    /// Called from the AppState persistence transaction so a domain mutation
    /// and every job it creates become visible atomically.
    static func ensureJobs(
        _ jobs: [DesiredJob],
        in db: OpaquePointer,
        afterEach: (Int) throws -> Void = { _ in }
    ) throws {
        guard !jobs.isEmpty else { return }
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
        let statement = try WorkflowSQLite.prepare(
            """
            INSERT INTO jobs(
                id,idempotency_key,kind,subject_id,input_version,occurrence_id,
                payload_version,payload,state,priority,resource_class,attempt,
                max_attempts,not_before,created_at,updated_at
            ) VALUES(?,?,?,?,?,?,?,?,'pending',?,?,0,?,?,?,?)
            ON CONFLICT(idempotency_key) DO NOTHING
            """,
            db: db
        )
        defer { sqlite3_finalize(statement) }
        let now = Date()
        for (index, job) in jobs.sorted(by: { $0.idempotencyKey < $1.idempotencyKey }).enumerated() {
            try WorkflowSQLite.bind(UUID().uuidString, 1, statement, db)
            try WorkflowSQLite.bind(job.idempotencyKey, 2, statement, db)
            try WorkflowSQLite.bind(job.kind.rawValue, 3, statement, db)
            try WorkflowSQLite.bind(job.subjectID.uuidString, 4, statement, db)
            try WorkflowSQLite.bind(job.inputVersion, 5, statement, db)
            try WorkflowSQLite.bind(job.occurrenceID, 6, statement, db)
            try WorkflowSQLite.bind(Int64(job.payloadVersion), 7, statement, db)
            try WorkflowSQLite.bind(job.payload, 8, statement, db)
            try WorkflowSQLite.bind(Int64(job.priority), 9, statement, db)
            try WorkflowSQLite.bind(job.resourceClass.rawValue, 10, statement, db)
            try WorkflowSQLite.bind(Int64(job.maxAttempts), 11, statement, db)
            try WorkflowSQLite.bind(now, 12, statement, db)
            try WorkflowSQLite.bind(now, 13, statement, db)
            try WorkflowSQLite.bind(now, 14, statement, db)
            try WorkflowSQLite.stepDone(statement, db)
            sqlite3_reset(statement)
            sqlite3_clear_bindings(statement)
            try afterEach(index)
        }
    }
}
