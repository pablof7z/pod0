import CSQLite3
import Foundation

extension JobStore {
    /// Complete, decode-only source inventory for the one-shot Rust cutover.
    func legacyScheduledAgentJobs() throws -> [WorkJob] {
        try withDatabase(publishChanges: false) { db in
            try legacyScheduledAgentJobs(db: db)
        }
    }

    func legacyScheduledAgentArtifacts() throws -> [LegacyScheduledAgentArtifactRow] {
        try withDatabase(publishChanges: false) { db in
            try legacyScheduledAgentArtifacts(db: db)
        }
    }

    func legacyScheduledAgentSourceIsRetired() throws -> Bool {
        try legacyScheduledAgentJobs().isEmpty
            && legacyScheduledAgentArtifacts().isEmpty
    }

    func legacyScheduledAgentJobs(db: OpaquePointer) throws -> [WorkJob] {
        let statement = try WorkflowSQLite.prepare(
            "SELECT \(Self.columns) FROM jobs WHERE kind='scheduledAgentRun' ORDER BY id",
            db: db
        )
        defer { sqlite3_finalize(statement) }
        return try readRows(statement).sorted { $0.id.uuidString < $1.id.uuidString }
    }

    func legacyScheduledAgentArtifacts(
        db: OpaquePointer
    ) throws -> [LegacyScheduledAgentArtifactRow] {
        let statement = try WorkflowSQLite.prepare(
            """
            SELECT kind,subject_id,input_version,output_version,content_hash,
                   location,origin,schema_version,integrity,verified_at,selected
            FROM artifacts WHERE kind='scheduledOutput'
            ORDER BY subject_id,input_version,output_version,id
            """,
            db: db
        )
        defer { sqlite3_finalize(statement) }
        let repository = ArtifactRepository(fileURL: fileURL)
        var rows: [LegacyScheduledAgentArtifactRow] = []
        while sqlite3_step(statement) == SQLITE_ROW {
            rows.append(LegacyScheduledAgentArtifactRow(
                record: try repository.read(statement),
                selected: sqlite3_column_int64(statement, 10) != 0
            ))
        }
        return rows
    }
}
