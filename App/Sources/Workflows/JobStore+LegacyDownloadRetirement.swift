import CSQLite3
import Foundation

extension JobStore {
    func retireLegacyDownloadWorkflows(
        matching backup: LegacyDownloadWorkflowBackup
    ) throws -> Bool {
        try withDatabase { db in
            try WorkflowSQLite.execute("BEGIN IMMEDIATE TRANSACTION", db)
            do {
                let currentJobs = try legacyDownloadJobs(db: db)
                    .filter { $0.kind == .download || $0.kind == .autoDownload }
                    .sorted { $0.id.uuidString < $1.id.uuidString }
                let currentArtifacts = try legacyDownloadArtifacts(db: db)
                guard currentJobs == backup.jobs,
                      currentArtifacts == backup.artifacts else {
                    try WorkflowSQLite.execute("COMMIT TRANSACTION", db)
                    return false
                }
                let deleteJobs = try WorkflowSQLite.prepare(
                    "DELETE FROM jobs WHERE kind IN ('download','autoDownload')",
                    db: db
                )
                defer { sqlite3_finalize(deleteJobs) }
                try WorkflowSQLite.stepDone(deleteJobs, db)

                let deleteArtifacts = try WorkflowSQLite.prepare(
                    "DELETE FROM artifacts WHERE kind IN ('downloadFile','autoDownloadDecision')",
                    db: db
                )
                defer { sqlite3_finalize(deleteArtifacts) }
                try WorkflowSQLite.stepDone(deleteArtifacts, db)
                try WorkflowSQLite.execute("COMMIT TRANSACTION", db)
                return true
            } catch {
                try? WorkflowSQLite.execute("ROLLBACK TRANSACTION", db)
                throw error
            }
        }
    }

    func legacyDownloadWorkflowsAreRetired() throws -> Bool {
        try withDatabase(publishChanges: false) { db in
            let statement = try WorkflowSQLite.prepare(
                """
                SELECT
                    (SELECT COUNT(*) FROM jobs
                     WHERE kind IN ('download','autoDownload'))
                    +
                    (SELECT COUNT(*) FROM artifacts
                     WHERE kind IN ('downloadFile','autoDownloadDecision'))
                """,
                db: db
            )
            defer { sqlite3_finalize(statement) }
            guard sqlite3_step(statement) == SQLITE_ROW else {
                throw JobStoreError.transitionRejected
            }
            return sqlite3_column_int64(statement, 0) == 0
        }
    }

    private func legacyDownloadJobs(db: OpaquePointer) throws -> [WorkJob] {
        let statement = try WorkflowSQLite.prepare(
            """
            SELECT \(Self.columns) FROM jobs
            WHERE kind IN ('download','autoDownload')
            ORDER BY created_at,id
            """,
            db: db
        )
        defer { sqlite3_finalize(statement) }
        return try readRows(statement)
    }

    private func legacyDownloadArtifacts(db: OpaquePointer) throws -> [ArtifactRecord] {
        let statement = try WorkflowSQLite.prepare(
            """
            SELECT kind,subject_id,input_version,output_version,content_hash,
                   location,origin,schema_version,integrity,verified_at
            FROM artifacts
            WHERE selected=1 AND kind IN ('downloadFile','autoDownloadDecision')
            """,
            db: db
        )
        defer { sqlite3_finalize(statement) }
        var records: [ArtifactRecord] = []
        while sqlite3_step(statement) == SQLITE_ROW {
            guard let kind = WorkflowSQLite.text(statement, 0).flatMap(ArtifactKind.init(rawValue:)),
                  let subjectID = WorkflowSQLite.text(statement, 1).flatMap(UUID.init(uuidString:)),
                  let inputVersion = WorkflowSQLite.text(statement, 2),
                  let outputVersion = WorkflowSQLite.text(statement, 3),
                  let contentHash = WorkflowSQLite.text(statement, 4),
                  let integrity = WorkflowSQLite.text(statement, 8)
                    .flatMap(ArtifactIntegrity.init(rawValue:)),
                  let verifiedAt = WorkflowSQLite.date(statement, 9)
            else { throw JobStoreError.corruptRow }
            records.append(ArtifactRecord(
                kind: kind,
                subjectID: subjectID,
                inputVersion: inputVersion,
                outputVersion: outputVersion,
                contentHash: contentHash,
                location: WorkflowSQLite.text(statement, 5),
                origin: WorkflowSQLite.text(statement, 6),
                schemaVersion: Int(sqlite3_column_int64(statement, 7)),
                integrity: integrity,
                verifiedAt: verifiedAt
            ))
        }
        return records.sorted { lhs, rhs in
            if lhs.subjectID != rhs.subjectID {
                return lhs.subjectID.uuidString < rhs.subjectID.uuidString
            }
            return lhs.outputVersion < rhs.outputVersion
        }
    }
}
