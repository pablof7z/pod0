import CSQLiteVec
import Foundation
import XCTest
@testable import Podcastr

final class WorkflowSchemaMigrationTests: XCTestCase {
    private var fileURL: URL!

    override func setUp() {
        super.setUp()
        fileURL = Persistence.episodeStoreURL(for: AppStateTestSupport.uniqueTempFileURL())
    }

    override func tearDown() {
        if let fileURL {
            for suffix in ["", "-wal", "-shm"] {
                try? FileManager.default.removeItem(
                    at: URL(fileURLWithPath: fileURL.path + suffix)
                )
            }
        }
        fileURL = nil
        super.tearDown()
    }

    func testLegacyJobsRetainOccurrenceDedupAndLeaseState() throws {
        let occurrenceID = "scheduled:2026-07-18T12:00:00Z"
        try raw { db in
            try WorkflowSQLite.execute(Self.jobsV0SQL, db)
            try WorkflowSQLite.execute(
                """
                INSERT INTO jobs VALUES(
                    '00000000-0000-0000-0000-000000000001','scheduledAgentRun',
                    '10000000-0000-0000-0000-000000000001','prompt-v1','\(occurrenceID)',
                    2,X'0102','running',80,'scheduledAgent',3,8,1000,
                    '20000000-0000-0000-0000-000000000001','old-owner',1100,
                    'provider','operation-7','submitted',NULL,'transient','waiting',900,950
                );
                INSERT INTO jobs VALUES(
                    '00000000-0000-0000-0000-000000000002','metadataIndex',
                    '10000000-0000-0000-0000-000000000002','episode-v1',NULL,
                    1,NULL,'succeeded',10,'embedding',1,4,1000,
                    NULL,NULL,NULL,NULL,NULL,NULL,'output-v1',NULL,NULL,800,990
                )
                """,
                db
            )
        }

        let jobs = try JobStore(fileURL: fileURL).allJobs()

        XCTAssertEqual(jobs.count, 2)
        let occurrence = try XCTUnwrap(jobs.first { $0.occurrenceID == occurrenceID })
        XCTAssertEqual(occurrence.idempotencyKey, occurrenceID)
        XCTAssertEqual(occurrence.state, .running)
        XCTAssertEqual(occurrence.attempt, 3)
        XCTAssertEqual(occurrence.leaseOwner, "old-owner")
        XCTAssertEqual(occurrence.externalOperationID, "operation-7")
        XCTAssertEqual(occurrence.payload, Data([1, 2]))
        XCTAssertEqual(
            jobs.first { $0.outputVersion == "output-v1" }?.idempotencyKey,
            "legacy:00000000-0000-0000-0000-000000000002"
        )
        XCTAssertEqual(try schemaVersion("jobs"), 1)
    }

    func testLegacyArtifactsRetainHistoryAndSelectNewestAvailableRow() throws {
        let subjectID = UUID(uuidString: "10000000-0000-0000-0000-000000000003")!
        try raw { db in
            try WorkflowSQLite.execute(Self.artifactsV0SQL, db)
            try WorkflowSQLite.execute(
                """
                INSERT INTO artifacts VALUES
                    (1,'transcript','\(subjectID.uuidString)','in-1','out-1','hash-1',NULL,'legacy',1,'available',100),
                    (2,'transcript','\(subjectID.uuidString)','in-2','out-2','hash-2','two','legacy',1,'available',200),
                    (3,'transcript','\(subjectID.uuidString)','in-3','out-3','hash-3','three','legacy',1,'stale',300)
                """,
                db
            )
        }

        let repository = ArtifactRepository(fileURL: fileURL)
        let current = try XCTUnwrap(repository.current(kind: .transcript, subjectID: subjectID))
        let history = try repository.history(kind: .transcript, subjectID: subjectID)

        XCTAssertEqual(current.outputVersion, "out-2")
        XCTAssertEqual(current.location, "two")
        XCTAssertEqual(history.map(\.outputVersion), ["out-3", "out-2", "out-1"])
        XCTAssertEqual(try schemaVersion("artifacts"), 1)
    }

    func testInterruptedJobMigrationRollsBackAndSucceedsAfterReopen() throws {
        try raw { db in
            try WorkflowSQLite.execute(Self.jobsV0SQL, db)
            try WorkflowSQLite.execute(
                """
                INSERT INTO jobs VALUES(
                    '00000000-0000-0000-0000-000000000004','download',
                    '10000000-0000-0000-0000-000000000004','download-v1',NULL,
                    1,NULL,'pending',0,'download',0,8,1000,
                    NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,900,900
                )
                """,
                db
            )
        }

        XCTAssertThrowsError(try raw { db in
            try WorkflowSchemaMigrations.ensureJobs(db) { step in
                if case .jobsLegacyRowsUpdated = step { throw InjectedInterruption() }
            }
        }) { error in
            XCTAssertTrue(error is InjectedInterruption)
        }
        try raw { db in
            XCTAssertFalse(try WorkflowSQLite.columnExists("idempotency_key", table: "jobs", db))
            XCTAssertEqual(try Self.count("jobs", db: db), 1)
            XCTAssertFalse(try WorkflowSQLite.tableExists("workflow_schema_versions", db))
        }

        let jobs = try JobStore(fileURL: fileURL).allJobs()
        XCTAssertEqual(jobs.map(\.idempotencyKey), [
            "legacy:00000000-0000-0000-0000-000000000004",
        ])
        XCTAssertEqual(try schemaVersion("jobs"), 1)
    }

    func testUnknownJobSchemaFailsClosedWithoutDeletingRows() throws {
        try raw { db in
            try WorkflowSQLite.execute("CREATE TABLE jobs(id TEXT, sentinel TEXT)", db)
            try WorkflowSQLite.execute("INSERT INTO jobs VALUES('legacy-id','keep-me')", db)
        }

        XCTAssertThrowsError(try JobStore(fileURL: fileURL).allJobs()) { error in
            guard case JobStoreError.unsupportedSchema(let component, _) = error else {
                return XCTFail("Unexpected error: \(error)")
            }
            XCTAssertEqual(component, "jobs")
        }
        try raw { db in
            XCTAssertEqual(try Self.count("jobs", db: db), 1)
            XCTAssertEqual(try Self.text("SELECT sentinel FROM jobs", db: db), "keep-me")
        }
    }

    func testFutureArtifactVersionFailsClosedWithoutDeletingRows() throws {
        try raw { db in
            try WorkflowSQLite.execute(Self.artifactsV0SQL, db)
            try WorkflowSQLite.execute(
                "INSERT INTO artifacts VALUES(1,'transcript','subject','in','out','hash',NULL,NULL,1,'available',100)",
                db
            )
            try WorkflowSQLite.execute(
                """
                CREATE TABLE workflow_schema_versions(
                    component TEXT PRIMARY KEY NOT NULL, version INTEGER NOT NULL
                );
                INSERT INTO workflow_schema_versions VALUES('artifacts',2)
                """,
                db
            )
        }

        XCTAssertThrowsError(
            try ArtifactRepository(fileURL: fileURL).all()
        ) { error in
            guard case JobStoreError.unsupportedSchema(let component, _) = error else {
                return XCTFail("Unexpected error: \(error)")
            }
            XCTAssertEqual(component, "artifacts")
        }
        try raw { db in
            XCTAssertEqual(try Self.count("artifacts", db: db), 1)
            XCTAssertFalse(try WorkflowSQLite.columnExists("selected", table: "artifacts", db))
            XCTAssertEqual(try schemaVersion("artifacts"), 2)
        }
    }

    func testRepeatedSchemaValidationDoesNotReportPhantomJobChanges() throws {
        let store = JobStore(fileURL: fileURL)
        let desired = DesiredJob(
            idempotencyKey: "schema-counting",
            kind: .metadataIndex,
            subjectID: UUID(),
            inputVersion: "v1",
            resourceClass: .embedding
        )

        XCTAssertEqual(try store.ensureJobs([desired]), 1)
        XCTAssertEqual(try store.ensureJobs([desired]), 0)
    }
}

private extension WorkflowSchemaMigrationTests {
    struct InjectedInterruption: Error {}

    func raw<T>(_ body: (OpaquePointer) throws -> T) throws -> T {
        try WorkflowSQLite.withDatabase(fileURL: fileURL, body)
    }

    func schemaVersion(_ component: String) throws -> Int? {
        try raw { db in
            let statement = try WorkflowSQLite.prepare(
                "SELECT version FROM workflow_schema_versions WHERE component=?",
                db: db
            )
            defer { sqlite3_finalize(statement) }
            try WorkflowSQLite.bind(component, 1, statement, db)
            guard sqlite3_step(statement) == SQLITE_ROW else { return nil }
            return Int(sqlite3_column_int64(statement, 0))
        }
    }

    static func count(_ table: String, db: OpaquePointer) throws -> Int {
        let statement = try WorkflowSQLite.prepare("SELECT COUNT(*) FROM \(table)", db: db)
        defer { sqlite3_finalize(statement) }
        guard sqlite3_step(statement) == SQLITE_ROW else { return -1 }
        return Int(sqlite3_column_int64(statement, 0))
    }

    static func text(_ sql: String, db: OpaquePointer) throws -> String? {
        let statement = try WorkflowSQLite.prepare(sql, db: db)
        defer { sqlite3_finalize(statement) }
        guard sqlite3_step(statement) == SQLITE_ROW else { return nil }
        return WorkflowSQLite.text(statement, 0)
    }

    static let jobsV0SQL = """
        CREATE TABLE jobs(
            id TEXT PRIMARY KEY NOT NULL,
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
        """

    static let artifactsV0SQL = """
        CREATE TABLE artifacts(
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            kind TEXT NOT NULL, subject_id TEXT NOT NULL,
            input_version TEXT NOT NULL, output_version TEXT NOT NULL,
            content_hash TEXT NOT NULL, location TEXT, origin TEXT,
            schema_version INTEGER NOT NULL, integrity TEXT NOT NULL,
            verified_at REAL NOT NULL,
            UNIQUE(kind,subject_id,input_version,output_version)
        )
        """
}
