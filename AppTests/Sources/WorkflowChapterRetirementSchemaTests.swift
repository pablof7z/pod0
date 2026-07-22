import CSQLite3
import Foundation
import XCTest
@testable import Podcastr

final class WorkflowChapterRetirementSchemaTests: XCTestCase {
    private var fileURL: URL!

    override func setUp() {
        super.setUp()
        fileURL = Persistence.episodeStoreURL(
            for: AppStateTestSupport.uniqueTempFileURL()
        )
    }

    override func tearDown() {
        AppStateTestSupport.disposeIsolatedStore(at: fileURL)
        fileURL = nil
        super.tearDown()
    }

    func testMarkerSchemaIsVersionedBeforeUse() throws {
        _ = try JobStore(fileURL: fileURL).allJobs()

        try raw { db in
            XCTAssertEqual(try schemaVersion(db), 1)
            XCTAssertTrue(try WorkflowSQLite.tableExists(
                "legacy_chapter_workflow_retirement",
                db
            ))
            XCTAssertEqual(try countMarkerRows(db), 0)
        }
    }

    func testUnknownSchemaFailsClosedWithoutDroppingEvidence() throws {
        _ = try JobStore(fileURL: fileURL).allJobs()
        try raw { db in
            try WorkflowSQLite.execute("DROP TABLE legacy_chapter_workflow_retirement", db)
            try WorkflowSQLite.execute(
                "CREATE TABLE legacy_chapter_workflow_retirement(singleton INTEGER, sentinel TEXT)",
                db
            )
            try WorkflowSQLite.execute(
                "INSERT INTO legacy_chapter_workflow_retirement VALUES(1,'keep-me')",
                db
            )
        }

        XCTAssertThrowsError(try JobStore(fileURL: fileURL).allJobs()) { error in
            guard case JobStoreError.unsupportedSchema(let component, _) = error else {
                return XCTFail("Unexpected error: \(error)")
            }
            XCTAssertEqual(component, "chapter_retirement")
        }
        try raw { db in
            let statement = try WorkflowSQLite.prepare(
                "SELECT sentinel FROM legacy_chapter_workflow_retirement",
                db: db
            )
            defer { sqlite3_finalize(statement) }
            XCTAssertEqual(
                sqlite3_step(statement) == SQLITE_ROW
                    ? WorkflowSQLite.text(statement, 0) : nil,
                "keep-me"
            )
        }
    }

    private func raw<T>(_ body: (OpaquePointer) throws -> T) throws -> T {
        try WorkflowSQLite.withDatabase(fileURL: fileURL, body)
    }

    private func schemaVersion(_ db: OpaquePointer) throws -> Int? {
        let statement = try WorkflowSQLite.prepare(
            "SELECT version FROM workflow_schema_versions WHERE component='chapter_retirement'",
            db: db
        )
        defer { sqlite3_finalize(statement) }
        guard sqlite3_step(statement) == SQLITE_ROW else { return nil }
        return Int(sqlite3_column_int64(statement, 0))
    }

    private func countMarkerRows(_ db: OpaquePointer) throws -> Int {
        let statement = try WorkflowSQLite.prepare(
            "SELECT COUNT(*) FROM legacy_chapter_workflow_retirement",
            db: db
        )
        defer { sqlite3_finalize(statement) }
        guard sqlite3_step(statement) == SQLITE_ROW else { return -1 }
        return Int(sqlite3_column_int64(statement, 0))
    }
}
