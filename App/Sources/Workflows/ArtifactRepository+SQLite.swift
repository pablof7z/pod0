import CSQLite3
import Foundation

extension ArtifactRepository {
    func withDatabase<T>(_ body: (OpaquePointer) throws -> T) throws -> T {
        try WorkflowSQLite.withDatabase(fileURL: fileURL) { db in
            try ensureSchema(db)
            return try body(db)
        }
    }

    func ensureSchema(_ db: OpaquePointer) throws {
        try WorkflowSchemaMigrations.ensureArtifacts(db)
    }

    func upsert(
        _ record: ArtifactRecord,
        db: OpaquePointer,
        beforeSelection: (@Sendable () throws -> Void)? = nil
    ) throws {
        let deselect = try WorkflowSQLite.prepare(
            "UPDATE artifacts SET selected=0, integrity='stale' WHERE kind=? AND subject_id=? AND selected=1",
            db: db
        )
        try WorkflowSQLite.bind(record.kind.rawValue, 1, deselect, db)
        try WorkflowSQLite.bind(record.subjectID.uuidString, 2, deselect, db)
        try WorkflowSQLite.stepDone(deselect, db)
        sqlite3_finalize(deselect)
        try beforeSelection?()
        let statement = try WorkflowSQLite.prepare(
            """
            INSERT INTO artifacts(
                kind,subject_id,input_version,output_version,content_hash,
                location,origin,schema_version,integrity,verified_at,selected
            ) VALUES(?,?,?,?,?,?,?,?,?,?,1)
            ON CONFLICT(kind,subject_id,input_version,output_version) DO UPDATE SET
                content_hash=excluded.content_hash, location=excluded.location,
                origin=excluded.origin, schema_version=excluded.schema_version,
                integrity=excluded.integrity, verified_at=excluded.verified_at,
                selected=1
            """,
            db: db
        )
        defer { sqlite3_finalize(statement) }
        try WorkflowSQLite.bind(record.kind.rawValue, 1, statement, db)
        try WorkflowSQLite.bind(record.subjectID.uuidString, 2, statement, db)
        try WorkflowSQLite.bind(record.inputVersion, 3, statement, db)
        try WorkflowSQLite.bind(record.outputVersion, 4, statement, db)
        try WorkflowSQLite.bind(record.contentHash, 5, statement, db)
        try WorkflowSQLite.bind(record.location, 6, statement, db)
        try WorkflowSQLite.bind(record.origin, 7, statement, db)
        try WorkflowSQLite.bind(Int64(record.schemaVersion), 8, statement, db)
        try WorkflowSQLite.bind(record.integrity.rawValue, 9, statement, db)
        try WorkflowSQLite.bind(record.verifiedAt, 10, statement, db)
        try WorkflowSQLite.stepDone(statement, db)
    }

    func read(_ statement: OpaquePointer) throws -> ArtifactRecord {
        guard let kind = WorkflowSQLite.text(statement, 0).flatMap(ArtifactKind.init(rawValue:)),
              let subject = WorkflowSQLite.text(statement, 1).flatMap(UUID.init(uuidString:)),
              let input = WorkflowSQLite.text(statement, 2),
              let output = WorkflowSQLite.text(statement, 3),
              let hash = WorkflowSQLite.text(statement, 4),
              let integrity = WorkflowSQLite.text(statement, 8).flatMap(ArtifactIntegrity.init(rawValue:)),
              let verified = WorkflowSQLite.date(statement, 9) else { throw JobStoreError.corruptRow }
        return ArtifactRecord(
            kind: kind, subjectID: subject, inputVersion: input,
            outputVersion: output, contentHash: hash,
            location: WorkflowSQLite.text(statement, 5),
            origin: WorkflowSQLite.text(statement, 6),
            schemaVersion: Int(sqlite3_column_int64(statement, 7)),
            integrity: integrity, verifiedAt: verified
        )
    }
}
