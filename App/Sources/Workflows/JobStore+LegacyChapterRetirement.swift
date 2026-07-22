import CSQLite3
import Foundation

struct LegacyChapterWorkflowRetirementMarker: Sendable, Equatable {
    static let schemaVersion = 1

    let schemaVersion: Int
    let modelSourceGeneration: UInt64
    let publisherSourceGeneration: UInt64
    let publisherSourceFingerprint: String
    let completedAt: Date

    init(
        modelSourceGeneration: UInt64,
        publisherSourceGeneration: UInt64,
        publisherSourceFingerprint: String,
        completedAt: Date
    ) {
        schemaVersion = Self.schemaVersion
        self.modelSourceGeneration = modelSourceGeneration
        self.publisherSourceGeneration = publisherSourceGeneration
        self.publisherSourceFingerprint = publisherSourceFingerprint
        self.completedAt = completedAt
    }

    static func == (
        lhs: LegacyChapterWorkflowRetirementMarker,
        rhs: LegacyChapterWorkflowRetirementMarker
    ) -> Bool {
        lhs.schemaVersion == rhs.schemaVersion
            && lhs.modelSourceGeneration == rhs.modelSourceGeneration
            && lhs.publisherSourceGeneration == rhs.publisherSourceGeneration
            && lhs.publisherSourceFingerprint == rhs.publisherSourceFingerprint
            && abs(lhs.completedAt.timeIntervalSince(rhs.completedAt)) < 0.000_001
    }
}

extension JobStore {
    /// Quarantined read surface for one-shot chapter retirement. Normal job
    /// APIs deliberately cannot decode these retired raw kind strings.
    func legacyChapterJobs(
        kind: LegacyChapterWorkflowJobKind
    ) throws -> [LegacyChapterWorkflowJob] {
        try withDatabase(publishChanges: false) { db in
            try legacyChapterJobs(kind: kind, db: db)
        }
    }

    func legacyChapterWorkflowRetirementMarker()
        throws -> LegacyChapterWorkflowRetirementMarker?
    {
        try withDatabase(publishChanges: false) { db in
            try legacyChapterWorkflowRetirementMarker(db: db)
        }
    }

    /// Model cutover deletes only the exact, durably backed-up legacy source.
    /// The final combined retirement marker is committed separately after the
    /// Rust model authority marker is verified and publisher rows are backed up.
    func removeLegacyChapterJobs(
        kind: LegacyChapterWorkflowJobKind,
        matching expected: [LegacyChapterWorkflowJob]
    ) throws -> Bool {
        try withDatabase { db in
            try WorkflowSQLite.execute("BEGIN IMMEDIATE TRANSACTION", db)
            do {
                let current = try legacyChapterJobs(kind: kind, db: db)
                guard current == expected.sorted(by: Self.sortLegacyJobs) else {
                    try WorkflowSQLite.execute("COMMIT TRANSACTION", db)
                    return false
                }
                let delete = try WorkflowSQLite.prepare(
                    "DELETE FROM jobs WHERE kind=?",
                    db: db
                )
                defer { sqlite3_finalize(delete) }
                try WorkflowSQLite.bind(kind.rawValue, 1, delete, db)
                try WorkflowSQLite.stepDone(delete, db)
                guard Int(sqlite3_changes(db)) == current.count else {
                    throw JobStoreError.transitionRejected
                }
                try WorkflowSQLite.execute("COMMIT TRANSACTION", db)
                return true
            } catch {
                try? WorkflowSQLite.execute("ROLLBACK TRANSACTION", db)
                throw error
            }
        }
    }

    /// Atomically proves model rows are gone, compares the complete publisher
    /// source, deletes only that exact source, and commits the final retirement
    /// marker in the same SQLite writer transaction.
    func commitLegacyChapterWorkflowRetirement(
        expectedPublisherJobs: [LegacyChapterWorkflowJob],
        marker: LegacyChapterWorkflowRetirementMarker
    ) throws -> Bool {
        try withDatabase { db in
            try WorkflowSQLite.execute("BEGIN IMMEDIATE TRANSACTION", db)
            do {
                guard try legacyChapterJobs(kind: .chapterArtifacts, db: db).isEmpty else {
                    try WorkflowSQLite.execute("COMMIT TRANSACTION", db)
                    return false
                }
                let existing = try legacyChapterWorkflowRetirementMarker(db: db)
                let current = try legacyChapterJobs(kind: .publisherChapters, db: db)
                let expected = expectedPublisherJobs.sorted(by: Self.sortLegacyJobs)
                guard current == expected else {
                    try WorkflowSQLite.execute("COMMIT TRANSACTION", db)
                    return false
                }
                if let existing {
                    let valid = existing == marker && current.isEmpty
                    try WorkflowSQLite.execute("COMMIT TRANSACTION", db)
                    return valid
                }

                let delete = try WorkflowSQLite.prepare(
                    "DELETE FROM jobs WHERE kind=?",
                    db: db
                )
                defer { sqlite3_finalize(delete) }
                try WorkflowSQLite.bind(
                    LegacyChapterWorkflowJobKind.publisherChapters.rawValue,
                    1,
                    delete,
                    db
                )
                try WorkflowSQLite.stepDone(delete, db)
                guard Int(sqlite3_changes(db)) == current.count else {
                    throw JobStoreError.transitionRejected
                }
                try insertLegacyChapterWorkflowRetirementMarker(marker, db: db)
                try WorkflowSQLite.execute("COMMIT TRANSACTION", db)
                return true
            } catch {
                try? WorkflowSQLite.execute("ROLLBACK TRANSACTION", db)
                throw error
            }
        }
    }

    func verifyLegacyChapterWorkflowRetirement(
        _ expected: LegacyChapterWorkflowRetirementMarker
    ) throws -> Bool {
        try withDatabase(publishChanges: false) { db in
            guard try legacyChapterWorkflowRetirementMarker(db: db) == expected else {
                return false
            }
            return try LegacyChapterWorkflowJobKind.allCasesForRetirement.allSatisfy {
                try legacyChapterJobs(kind: $0, db: db).isEmpty
            }
        }
    }
}

private extension JobStore {
    static func sortLegacyJobs(
        _ lhs: LegacyChapterWorkflowJob,
        _ rhs: LegacyChapterWorkflowJob
    ) -> Bool {
        lhs.id.uuidString < rhs.id.uuidString
    }

    func legacyChapterJobs(
        kind: LegacyChapterWorkflowJobKind,
        db: OpaquePointer
    ) throws -> [LegacyChapterWorkflowJob] {
        let statement = try WorkflowSQLite.prepare(
            "SELECT \(Self.columns) FROM jobs WHERE kind=? ORDER BY id",
            db: db
        )
        defer { sqlite3_finalize(statement) }
        try WorkflowSQLite.bind(kind.rawValue, 1, statement, db)
        return try readLegacyChapterRows(statement, expectedKind: kind)
    }

    func readLegacyChapterRows(
        _ statement: OpaquePointer,
        expectedKind: LegacyChapterWorkflowJobKind
    ) throws -> [LegacyChapterWorkflowJob] {
        var jobs: [LegacyChapterWorkflowJob] = []
        while sqlite3_step(statement) == SQLITE_ROW {
            guard let id = WorkflowSQLite.text(statement, 0).flatMap(UUID.init(uuidString:)),
                  let key = WorkflowSQLite.text(statement, 1),
                  WorkflowSQLite.text(statement, 2) == expectedKind.rawValue,
                  let subject = WorkflowSQLite.text(statement, 3).flatMap(UUID.init(uuidString:)),
                  let input = WorkflowSQLite.text(statement, 4),
                  let state = WorkflowSQLite.text(statement, 8).flatMap(WorkJobState.init(rawValue:)),
                  let resource = WorkflowSQLite.text(statement, 10)
                    .flatMap(WorkResourceClass.init(rawValue:)),
                  let notBefore = WorkflowSQLite.date(statement, 13),
                  let createdAt = WorkflowSQLite.date(statement, 23),
                  let updatedAt = WorkflowSQLite.date(statement, 24)
            else { throw JobStoreError.corruptRow }
            jobs.append(LegacyChapterWorkflowJob(
                id: id, idempotencyKey: key, kind: expectedKind, subjectID: subject,
                inputVersion: input, occurrenceID: WorkflowSQLite.text(statement, 5),
                payloadVersion: Int(sqlite3_column_int64(statement, 6)),
                payload: WorkflowSQLite.data(statement, 7), state: state,
                priority: Int(sqlite3_column_int64(statement, 9)), resourceClass: resource,
                attempt: Int(sqlite3_column_int64(statement, 11)),
                maxAttempts: Int(sqlite3_column_int64(statement, 12)),
                notBefore: notBefore,
                leaseToken: WorkflowSQLite.text(statement, 14).flatMap(UUID.init(uuidString:)),
                leaseOwner: WorkflowSQLite.text(statement, 15),
                leaseExpiresAt: WorkflowSQLite.date(statement, 16),
                externalProvider: WorkflowSQLite.text(statement, 17),
                externalOperationID: WorkflowSQLite.text(statement, 18),
                externalOperationState: WorkflowSQLite.text(statement, 19),
                outputVersion: WorkflowSQLite.text(statement, 20),
                lastErrorClass: WorkflowSQLite.text(statement, 21)
                    .flatMap(JobErrorClass.init(rawValue:)),
                lastErrorMessage: WorkflowSQLite.text(statement, 22),
                createdAt: createdAt, updatedAt: updatedAt
            ))
        }
        return jobs.sorted(by: Self.sortLegacyJobs)
    }

    func legacyChapterWorkflowRetirementMarker(
        db: OpaquePointer
    ) throws -> LegacyChapterWorkflowRetirementMarker? {
        let statement = try WorkflowSQLite.prepare(
            """
            SELECT schema_version,model_source_generation,publisher_source_generation,
                   publisher_source_fingerprint,completed_at
            FROM legacy_chapter_workflow_retirement WHERE singleton=1
            """,
            db: db
        )
        defer { sqlite3_finalize(statement) }
        guard sqlite3_step(statement) == SQLITE_ROW else { return nil }
        let schemaVersion = Int(sqlite3_column_int64(statement, 0))
        let model = sqlite3_column_int64(statement, 1)
        let publisher = sqlite3_column_int64(statement, 2)
        guard schemaVersion == LegacyChapterWorkflowRetirementMarker.schemaVersion,
              model > 0, publisher > 0,
              let fingerprint = WorkflowSQLite.text(statement, 3),
              fingerprint.count == 64,
              let completedAt = WorkflowSQLite.date(statement, 4),
              sqlite3_step(statement) == SQLITE_DONE
        else { throw JobStoreError.corruptRow }
        return LegacyChapterWorkflowRetirementMarker(
            modelSourceGeneration: UInt64(model),
            publisherSourceGeneration: UInt64(publisher),
            publisherSourceFingerprint: fingerprint,
            completedAt: completedAt
        )
    }

    func insertLegacyChapterWorkflowRetirementMarker(
        _ marker: LegacyChapterWorkflowRetirementMarker,
        db: OpaquePointer
    ) throws {
        guard marker.modelSourceGeneration <= UInt64(Int64.max),
              marker.publisherSourceGeneration <= UInt64(Int64.max)
        else { throw JobStoreError.corruptRow }
        let statement = try WorkflowSQLite.prepare(
            """
            INSERT INTO legacy_chapter_workflow_retirement(
                singleton,schema_version,model_source_generation,
                publisher_source_generation,publisher_source_fingerprint,completed_at
            ) VALUES(1,?,?,?,?,?)
            """,
            db: db
        )
        defer { sqlite3_finalize(statement) }
        try WorkflowSQLite.bind(Int64(marker.schemaVersion), 1, statement, db)
        try WorkflowSQLite.bind(Int64(marker.modelSourceGeneration), 2, statement, db)
        try WorkflowSQLite.bind(Int64(marker.publisherSourceGeneration), 3, statement, db)
        try WorkflowSQLite.bind(marker.publisherSourceFingerprint, 4, statement, db)
        try WorkflowSQLite.bind(marker.completedAt, 5, statement, db)
        try WorkflowSQLite.stepDone(statement, db)
    }
}

private extension LegacyChapterWorkflowJobKind {
    static let allCasesForRetirement: [Self] = [.publisherChapters, .chapterArtifacts]
}
