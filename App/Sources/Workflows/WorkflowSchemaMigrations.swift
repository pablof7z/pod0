import CSQLiteVec
import Foundation

enum WorkflowSchemaMigrationStep {
    case jobsLegacyRowsUpdated
    case artifactsLegacyRowsUpdated
}

enum WorkflowSchemaMigrations {
    private static let metadataTable = "workflow_schema_versions"
    private static let currentVersion = 1

    private static let jobColumnsV0: Set<String> = [
        "id", "kind", "subject_id", "input_version", "occurrence_id",
        "payload_version", "payload", "state", "priority", "resource_class",
        "attempt", "max_attempts", "not_before", "lease_token", "lease_owner",
        "lease_expires_at", "external_provider", "external_operation_id",
        "external_operation_state", "output_version", "last_error_class",
        "last_error_message", "created_at", "updated_at",
    ]

    private static let artifactColumnsV0: Set<String> = [
        "id", "kind", "subject_id", "input_version", "output_version",
        "content_hash", "location", "origin", "schema_version", "integrity",
        "verified_at",
    ]

    static func ensureJobs(
        _ db: OpaquePointer,
        afterMigrationStep: (WorkflowSchemaMigrationStep) throws -> Void = { _ in }
    ) throws {
        try migrate(component: "jobs", db: db) { recordedVersion in
            guard try WorkflowSQLite.tableExists("jobs", db) else {
                guard recordedVersion == nil else {
                    throw unsupported("jobs", "version metadata exists but the table is missing")
                }
                try createJobs(db)
                try setVersion(currentVersion, component: "jobs", db: db)
                return
            }

            let columns = try columnNames(table: "jobs", db: db)
            if columns == jobColumnsV0 {
                guard recordedVersion == nil || recordedVersion == 0 else {
                    throw unsupported("jobs", "legacy columns conflict with version \(recordedVersion!)")
                }
                try validateLegacyJobKeys(db)
                try WorkflowSQLite.execute("ALTER TABLE jobs ADD COLUMN idempotency_key TEXT", db)
                try WorkflowSQLite.execute(
                    """
                    UPDATE jobs SET idempotency_key=CASE
                        WHEN occurrence_id IS NOT NULL AND occurrence_id <> '' THEN occurrence_id
                        ELSE 'legacy:' || id
                    END
                    """,
                    db
                )
                try afterMigrationStep(.jobsLegacyRowsUpdated)
            } else if columns != jobColumnsV0.union(["idempotency_key"]) {
                throw unsupported("jobs", "unrecognized columns: \(columns.sorted().joined(separator: ","))")
            } else if let recordedVersion, recordedVersion != currentVersion {
                throw unsupported("jobs", "columns are current but recorded version is \(recordedVersion)")
            }

            try validateCurrentJobKeys(db)
            try createJobIndexesAndGuards(db)
            try setVersion(currentVersion, component: "jobs", db: db)
        }
    }

    static func ensureArtifacts(
        _ db: OpaquePointer,
        afterMigrationStep: (WorkflowSchemaMigrationStep) throws -> Void = { _ in }
    ) throws {
        try migrate(component: "artifacts", db: db) { recordedVersion in
            guard try WorkflowSQLite.tableExists("artifacts", db) else {
                guard recordedVersion == nil else {
                    throw unsupported("artifacts", "version metadata exists but the table is missing")
                }
                try createArtifacts(db)
                try setVersion(currentVersion, component: "artifacts", db: db)
                return
            }

            let columns = try columnNames(table: "artifacts", db: db)
            if columns == artifactColumnsV0 {
                guard recordedVersion == nil || recordedVersion == 0 else {
                    throw unsupported("artifacts", "legacy columns conflict with version \(recordedVersion!)")
                }
                try validateArtifactIdentity(db)
                try WorkflowSQLite.execute(
                    "ALTER TABLE artifacts ADD COLUMN selected INTEGER NOT NULL DEFAULT 0",
                    db
                )
                try WorkflowSQLite.execute(
                    """
                    UPDATE artifacts AS candidate SET selected=1
                    WHERE candidate.id=(
                        SELECT winner.id FROM artifacts AS winner
                        WHERE winner.kind=candidate.kind AND winner.subject_id=candidate.subject_id
                        ORDER BY CASE winner.integrity WHEN 'available' THEN 0 ELSE 1 END,
                                 winner.verified_at DESC, winner.id DESC
                        LIMIT 1
                    )
                    """,
                    db
                )
                try afterMigrationStep(.artifactsLegacyRowsUpdated)
            } else if columns != artifactColumnsV0.union(["selected"]) {
                throw unsupported("artifacts", "unrecognized columns: \(columns.sorted().joined(separator: ","))")
            } else if let recordedVersion, recordedVersion != currentVersion {
                throw unsupported("artifacts", "columns are current but recorded version is \(recordedVersion)")
            }

            try validateArtifactIdentity(db)
            try createArtifactIndexes(db)
            try setVersion(currentVersion, component: "artifacts", db: db)
        }
    }
}

private extension WorkflowSchemaMigrations {
    static func migrate(
        component: String,
        db: OpaquePointer,
        body: (Int?) throws -> Void
    ) throws {
        let savepoint = "workflow_\(component)_schema"
        try WorkflowSQLite.execute("SAVEPOINT \(savepoint)", db)
        do {
            try ensureMetadataTable(db)
            try body(try version(component: component, db: db))
            try WorkflowSQLite.execute("RELEASE SAVEPOINT \(savepoint)", db)
        } catch {
            try? WorkflowSQLite.execute("ROLLBACK TO SAVEPOINT \(savepoint)", db)
            try? WorkflowSQLite.execute("RELEASE SAVEPOINT \(savepoint)", db)
            throw error
        }
    }

    static func ensureMetadataTable(_ db: OpaquePointer) throws {
        if try WorkflowSQLite.tableExists(metadataTable, db) {
            let columns = try columnNames(table: metadataTable, db: db)
            guard columns == ["component", "version"] else {
                throw unsupported("metadata", "unrecognized columns: \(columns.sorted().joined(separator: ","))")
            }
            return
        }
        try WorkflowSQLite.execute(
            """
            CREATE TABLE workflow_schema_versions(
                component TEXT PRIMARY KEY NOT NULL,
                version INTEGER NOT NULL
            )
            """,
            db
        )
    }

    static func version(component: String, db: OpaquePointer) throws -> Int? {
        let statement = try WorkflowSQLite.prepare(
            "SELECT version FROM workflow_schema_versions WHERE component=?",
            db: db
        )
        defer { sqlite3_finalize(statement) }
        try WorkflowSQLite.bind(component, 1, statement, db)
        guard sqlite3_step(statement) == SQLITE_ROW else { return nil }
        let value = Int(sqlite3_column_int64(statement, 0))
        guard sqlite3_step(statement) == SQLITE_DONE else {
            throw unsupported("metadata", "duplicate version rows for \(component)")
        }
        guard value <= currentVersion else {
            throw unsupported(component, "future schema version \(value)")
        }
        return value
    }

    static func setVersion(_ version: Int, component: String, db: OpaquePointer) throws {
        let statement = try WorkflowSQLite.prepare(
            """
            INSERT INTO workflow_schema_versions(component,version) VALUES(?,?)
            ON CONFLICT(component) DO UPDATE SET version=excluded.version
            WHERE workflow_schema_versions.version <> excluded.version
            """,
            db: db
        )
        defer { sqlite3_finalize(statement) }
        try WorkflowSQLite.bind(component, 1, statement, db)
        try WorkflowSQLite.bind(Int64(version), 2, statement, db)
        try WorkflowSQLite.stepDone(statement, db)
    }

    static func columnNames(table: String, db: OpaquePointer) throws -> Set<String> {
        let statement = try WorkflowSQLite.prepare("PRAGMA table_info(\(table))", db: db)
        defer { sqlite3_finalize(statement) }
        var columns: Set<String> = []
        while sqlite3_step(statement) == SQLITE_ROW {
            if let name = WorkflowSQLite.text(statement, 1) { columns.insert(name) }
        }
        return columns
    }

    static func validateLegacyJobKeys(_ db: OpaquePointer) throws {
        try rejectRows(
            """
            SELECT CASE WHEN occurrence_id IS NOT NULL AND occurrence_id <> ''
                THEN occurrence_id ELSE 'legacy:' || id END AS migration_key
            FROM jobs GROUP BY migration_key
            HAVING migration_key IS NULL OR COUNT(*) > 1
            LIMIT 1
            """,
            component: "jobs",
            detail: "legacy rows cannot be assigned unique idempotency keys",
            db: db
        )
    }

    static func validateCurrentJobKeys(_ db: OpaquePointer) throws {
        try rejectRows(
            "SELECT 1 FROM jobs GROUP BY idempotency_key HAVING idempotency_key IS NULL OR COUNT(*) > 1 LIMIT 1",
            component: "jobs",
            detail: "idempotency keys are null or duplicated",
            db: db
        )
    }

    static func validateArtifactIdentity(_ db: OpaquePointer) throws {
        try rejectRows(
            """
            SELECT 1 FROM artifacts
            GROUP BY kind,subject_id,input_version,output_version
            HAVING kind IS NULL OR subject_id IS NULL OR input_version IS NULL
                OR output_version IS NULL OR COUNT(*) > 1
            LIMIT 1
            """,
            component: "artifacts",
            detail: "artifact identities are null or duplicated",
            db: db
        )
    }

    static func rejectRows(
        _ sql: String,
        component: String,
        detail: String,
        db: OpaquePointer
    ) throws {
        let statement = try WorkflowSQLite.prepare(sql, db: db)
        defer { sqlite3_finalize(statement) }
        if sqlite3_step(statement) == SQLITE_ROW { throw unsupported(component, detail) }
    }

    static func unsupported(_ component: String, _ detail: String) -> JobStoreError {
        .unsupportedSchema(component: component, detail: detail)
    }
}

private extension WorkflowSchemaMigrations {
    static func createJobs(_ db: OpaquePointer) throws {
        try WorkflowSQLite.execute(
            """
            CREATE TABLE jobs(
                id TEXT PRIMARY KEY NOT NULL, idempotency_key TEXT UNIQUE NOT NULL,
                kind TEXT NOT NULL, subject_id TEXT NOT NULL, input_version TEXT NOT NULL,
                occurrence_id TEXT, payload_version INTEGER NOT NULL, payload BLOB,
                state TEXT NOT NULL, priority INTEGER NOT NULL, resource_class TEXT NOT NULL,
                attempt INTEGER NOT NULL, max_attempts INTEGER NOT NULL, not_before REAL NOT NULL,
                lease_token TEXT, lease_owner TEXT, lease_expires_at REAL,
                external_provider TEXT, external_operation_id TEXT, external_operation_state TEXT,
                output_version TEXT, last_error_class TEXT, last_error_message TEXT,
                created_at REAL NOT NULL, updated_at REAL NOT NULL
            )
            """,
            db
        )
        try createJobIndexesAndGuards(db)
    }

    static func createJobIndexesAndGuards(_ db: OpaquePointer) throws {
        try WorkflowSQLite.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS jobs_idempotency_key_v1 ON jobs(idempotency_key)", db
        )
        try WorkflowSQLite.execute(
            "CREATE INDEX IF NOT EXISTS jobs_due_v2 ON jobs(resource_class,state,not_before,priority)", db
        )
        try WorkflowSQLite.execute(
            """
            CREATE TRIGGER IF NOT EXISTS jobs_idempotency_key_insert_v1
            BEFORE INSERT ON jobs WHEN NEW.idempotency_key IS NULL
            BEGIN SELECT RAISE(ABORT, 'jobs.idempotency_key must not be null'); END
            """,
            db
        )
        try WorkflowSQLite.execute(
            """
            CREATE TRIGGER IF NOT EXISTS jobs_idempotency_key_update_v1
            BEFORE UPDATE OF idempotency_key ON jobs WHEN NEW.idempotency_key IS NULL
            BEGIN SELECT RAISE(ABORT, 'jobs.idempotency_key must not be null'); END
            """,
            db
        )
    }

    static func createArtifacts(_ db: OpaquePointer) throws {
        try WorkflowSQLite.execute(
            """
            CREATE TABLE artifacts(
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                kind TEXT NOT NULL, subject_id TEXT NOT NULL,
                input_version TEXT NOT NULL, output_version TEXT NOT NULL,
                content_hash TEXT NOT NULL, location TEXT, origin TEXT,
                schema_version INTEGER NOT NULL, integrity TEXT NOT NULL,
                verified_at REAL NOT NULL, selected INTEGER NOT NULL,
                UNIQUE(kind,subject_id,input_version,output_version)
            )
            """,
            db
        )
        try createArtifactIndexes(db)
    }

    static func createArtifactIndexes(_ db: OpaquePointer) throws {
        try WorkflowSQLite.execute(
            """
            CREATE UNIQUE INDEX IF NOT EXISTS artifacts_identity_v1
            ON artifacts(kind,subject_id,input_version,output_version)
            """,
            db
        )
        try WorkflowSQLite.execute(
            """
            CREATE UNIQUE INDEX IF NOT EXISTS artifacts_selected_v2
            ON artifacts(kind,subject_id) WHERE selected=1
            """,
            db
        )
    }
}
