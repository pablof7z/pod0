import CSQLiteVec
import CryptoKit
import Foundation

enum ArtifactKind: String, CaseIterable, Codable, Sendable {
    case downloadFile
    case transcript
    case semanticIndex
    case metadataIndex
    case chapters
    case adSegments
    case scheduledOutput
    case notificationDelivery
    case feedDiscovery
    case autoDownloadDecision
}

enum ArtifactIntegrity: String, Codable, Sendable {
    case available
    case stale
    case corrupt
}

struct ArtifactRecord: Sendable, Equatable {
    let kind: ArtifactKind
    let subjectID: UUID
    let inputVersion: String
    let outputVersion: String
    let contentHash: String
    let location: String?
    let origin: String?
    let schemaVersion: Int
    let integrity: ArtifactIntegrity
    let verifiedAt: Date
}

struct ArtifactRepository: Sendable {
    let fileURL: URL
    private let adoptFaultInjector: (@Sendable () throws -> Void)?

    init(
        fileURL: URL,
        adoptFaultInjector: (@Sendable () throws -> Void)? = nil
    ) {
        self.fileURL = fileURL
        self.adoptFaultInjector = adoptFaultInjector
    }

    func current(kind: ArtifactKind, subjectID: UUID) throws -> ArtifactRecord? {
        try withDatabase { db in
            let statement = try WorkflowSQLite.prepare(
                """
                SELECT kind,subject_id,input_version,output_version,content_hash,
                       location,origin,schema_version,integrity,verified_at
                FROM artifacts WHERE kind=? AND subject_id=? AND selected=1
                """,
                db: db
            )
            defer { sqlite3_finalize(statement) }
            try WorkflowSQLite.bind(kind.rawValue, 1, statement, db)
            try WorkflowSQLite.bind(subjectID.uuidString, 2, statement, db)
            guard sqlite3_step(statement) == SQLITE_ROW else { return nil }
            return try read(statement)
        }
    }

    func all() throws -> [ArtifactRecord] {
        try withDatabase { db in
            let statement = try WorkflowSQLite.prepare(
                """
                SELECT kind,subject_id,input_version,output_version,content_hash,
                       location,origin,schema_version,integrity,verified_at
                FROM artifacts WHERE selected=1 ORDER BY kind,subject_id
                """,
                db: db
            )
            defer { sqlite3_finalize(statement) }
            var records: [ArtifactRecord] = []
            while sqlite3_step(statement) == SQLITE_ROW { records.append(try read(statement)) }
            return records
        }
    }

    func history(kind: ArtifactKind, subjectID: UUID) throws -> [ArtifactRecord] {
        try withDatabase { db in
            let statement = try WorkflowSQLite.prepare(
                """
                SELECT kind,subject_id,input_version,output_version,content_hash,
                       location,origin,schema_version,integrity,verified_at
                FROM artifacts WHERE kind=? AND subject_id=?
                ORDER BY verified_at DESC,id DESC
                """,
                db: db
            )
            defer { sqlite3_finalize(statement) }
            try WorkflowSQLite.bind(kind.rawValue, 1, statement, db)
            try WorkflowSQLite.bind(subjectID.uuidString, 2, statement, db)
            var records: [ArtifactRecord] = []
            while sqlite3_step(statement) == SQLITE_ROW { records.append(try read(statement)) }
            return records
        }
    }

    /// Selects a verified artifact and succeeds its producing job in one
    /// fenced SQLite transaction. A late attempt updates nothing.
    func commit(
        _ record: ArtifactRecord,
        completingJobID jobID: UUID,
        leaseToken: UUID
    ) throws {
        try commit([record], completingJobID: jobID, leaseToken: leaseToken)
    }

    func commit(
        _ records: [ArtifactRecord],
        completingJobID jobID: UUID,
        leaseToken: UUID
    ) throws {
        guard let primary = records.first else { throw JobStoreError.corruptRow }
        try requireNativeWritable(records)
        try withDatabase { db in
            try WorkflowSQLite.execute("BEGIN IMMEDIATE TRANSACTION", db)
            do {
                let job = try WorkflowSQLite.prepare(
                    """
                    SELECT 1 FROM jobs WHERE id=? AND lease_token=?
                      AND state IN ('leased','running')
                    """,
                    db: db
                )
                try WorkflowSQLite.bind(jobID.uuidString, 1, job, db)
                try WorkflowSQLite.bind(leaseToken.uuidString, 2, job, db)
                let ownsLease = sqlite3_step(job) == SQLITE_ROW
                sqlite3_finalize(job)
                guard ownsLease else { throw JobStoreError.transitionRejected }

                for record in records { try upsert(record, db: db) }
                let complete = try WorkflowSQLite.prepare(
                    """
                    UPDATE jobs SET state='succeeded', output_version=?, lease_token=NULL,
                        lease_owner=NULL, lease_expires_at=NULL, updated_at=?
                    WHERE id=? AND lease_token=? AND state IN ('leased','running')
                    """,
                    db: db
                )
                try WorkflowSQLite.bind(primary.outputVersion, 1, complete, db)
                try WorkflowSQLite.bind(Date(), 2, complete, db)
                try WorkflowSQLite.bind(jobID.uuidString, 3, complete, db)
                try WorkflowSQLite.bind(leaseToken.uuidString, 4, complete, db)
                try WorkflowSQLite.stepDone(complete, db)
                sqlite3_finalize(complete)
                guard sqlite3_changes(db) == 1 else { throw JobStoreError.transitionRejected }
                try WorkflowSQLite.execute("COMMIT TRANSACTION", db)
            } catch {
                try? WorkflowSQLite.execute("ROLLBACK TRANSACTION", db)
                throw error
            }
        }
        WorkflowJobChangeSignal.post(fileURL: fileURL)
    }

    func completeWithoutArtifact(
        outputVersion: String,
        completingJobID jobID: UUID,
        leaseToken: UUID
    ) throws {
        try withDatabase { db in
            try WorkflowSQLite.execute("BEGIN IMMEDIATE TRANSACTION", db)
            do {
                let complete = try WorkflowSQLite.prepare(
                    """
                    UPDATE jobs SET state='succeeded', output_version=?, lease_token=NULL,
                        lease_owner=NULL, lease_expires_at=NULL, updated_at=?
                    WHERE id=? AND lease_token=? AND state IN ('leased','running')
                    """,
                    db: db
                )
                defer { sqlite3_finalize(complete) }
                try WorkflowSQLite.bind(outputVersion, 1, complete, db)
                try WorkflowSQLite.bind(Date(), 2, complete, db)
                try WorkflowSQLite.bind(jobID.uuidString, 3, complete, db)
                try WorkflowSQLite.bind(leaseToken.uuidString, 4, complete, db)
                try WorkflowSQLite.stepDone(complete, db)
                guard sqlite3_changes(db) == 1 else {
                    throw JobStoreError.transitionRejected
                }
                try WorkflowSQLite.execute("COMMIT TRANSACTION", db)
            } catch {
                try? WorkflowSQLite.execute("ROLLBACK TRANSACTION", db)
                throw error
            }
        }
        WorkflowJobChangeSignal.post(fileURL: fileURL)
    }

    /// Records a verified artifact found by reconciliation. This is used only
    /// for adoption when no active attempt owns the already-written output.
    func adopt(_ record: ArtifactRecord) throws {
        try requireNativeWritable([record])
        try withDatabase { db in
            try WorkflowSQLite.execute("BEGIN IMMEDIATE TRANSACTION", db)
            do {
                try upsert(record, db: db, beforeSelection: adoptFaultInjector)
                try WorkflowSQLite.execute("COMMIT TRANSACTION", db)
            } catch {
                try? WorkflowSQLite.execute("ROLLBACK TRANSACTION", db)
                throw error
            }
        }
    }

    func markIntegrity(
        kind: ArtifactKind,
        subjectID: UUID,
        integrity: ArtifactIntegrity
    ) throws {
        try requireNativeWritable(kind: kind)
        try withDatabase { db in
            let statement = try WorkflowSQLite.prepare(
                "UPDATE artifacts SET integrity=?, verified_at=? WHERE kind=? AND subject_id=? AND selected=1",
                db: db
            )
            defer { sqlite3_finalize(statement) }
            try WorkflowSQLite.bind(integrity.rawValue, 1, statement, db)
            try WorkflowSQLite.bind(Date(), 2, statement, db)
            try WorkflowSQLite.bind(kind.rawValue, 3, statement, db)
            try WorkflowSQLite.bind(subjectID.uuidString, 4, statement, db)
            try WorkflowSQLite.stepDone(statement, db)
        }
    }

    static func hash(_ data: Data) -> String {
        SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }

    static func version(parts: [String]) -> String {
        hash(Data(parts.joined(separator: "\u{1f}").utf8))
    }
}
