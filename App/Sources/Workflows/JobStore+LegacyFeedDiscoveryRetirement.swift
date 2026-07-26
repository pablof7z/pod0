import CSQLite3
import Foundation

extension JobStore {
    func legacyFeedDiscoveryJobs() throws -> [LegacyFeedDiscoveryWorkJob] {
        try withDatabase(publishChanges: false) { db in
            try legacyFeedDiscoveryJobs(db: db)
        }
    }

    func legacyFeedDiscoveryArtifacts() throws -> [LegacyFeedDiscoveryArtifactRecord] {
        try withDatabase(publishChanges: false) { db in
            try legacyFeedDiscoveryArtifacts(db: db)
        }
    }

    func retireLegacyFeedDiscovery(
        matching backup: LegacyFeedDiscoveryWorkflowBackup,
        sourceGeneration: UInt64,
        sourceDigest: String
    ) throws -> Bool {
        try withDatabase { db in
            try WorkflowSQLite.execute("BEGIN IMMEDIATE TRANSACTION", db)
            do {
                try WorkflowSchemaMigrations.ensureFeedDiscoveryRetirement(db)
                if try retirementMatches(
                    db: db,
                    generation: sourceGeneration,
                    digest: sourceDigest,
                    jobCount: backup.jobs.count,
                    artifactCount: backup.artifacts.count
                ) {
                    try WorkflowSQLite.execute("COMMIT TRANSACTION", db)
                    return try legacyFeedDiscoverySourceIsRetired()
                }
                guard try legacyFeedDiscoveryJobs(db: db) == backup.jobs,
                      try legacyFeedDiscoveryArtifacts(db: db) == backup.artifacts
                else {
                    try WorkflowSQLite.execute("COMMIT TRANSACTION", db)
                    return false
                }
                try deleteLegacyFeedDiscoveryRows(db)
                try insertRetirement(
                    db: db,
                    generation: sourceGeneration,
                    digest: sourceDigest,
                    jobCount: backup.jobs.count,
                    artifactCount: backup.artifacts.count
                )
                try WorkflowSQLite.execute("COMMIT TRANSACTION", db)
                return true
            } catch {
                try? WorkflowSQLite.execute("ROLLBACK TRANSACTION", db)
                throw error
            }
        }
    }

    func legacyFeedDiscoverySourceIsRetired() throws -> Bool {
        try withDatabase(publishChanges: false) { db in
            try WorkflowSchemaMigrations.ensureFeedDiscoveryRetirement(db)
            let statement = try WorkflowSQLite.prepare(
                """
                SELECT
                  (SELECT COUNT(*) FROM legacy_feed_discovery_retirement)
                  + CASE WHEN (
                    SELECT COUNT(*) FROM jobs
                    WHERE kind IN ('feedDiscovery','newEpisodeNotification')
                  )=0 AND (
                    SELECT COUNT(*) FROM artifacts
                    WHERE kind IN ('feedDiscovery','notificationDelivery')
                  )=0 THEN 1 ELSE 0 END
                """,
                db: db
            )
            defer { sqlite3_finalize(statement) }
            guard sqlite3_step(statement) == SQLITE_ROW else {
                throw JobStoreError.transitionRejected
            }
            return sqlite3_column_int64(statement, 0) == 2
        }
    }

    func legacyFeedDiscoveryAuthorityIsRetired() throws -> Bool {
        try withDatabase(publishChanges: false) { db in
            try WorkflowSchemaMigrations.ensureFeedDiscoveryRetirement(db)
            let statement = try WorkflowSQLite.prepare(
                "SELECT COUNT(*) FROM legacy_feed_discovery_retirement",
                db: db
            )
            defer { sqlite3_finalize(statement) }
            guard sqlite3_step(statement) == SQLITE_ROW else {
                throw JobStoreError.transitionRejected
            }
            return sqlite3_column_int64(statement, 0) == 1
        }
    }
}

private extension JobStore {
    func legacyFeedDiscoveryJobs(
        db: OpaquePointer
    ) throws -> [LegacyFeedDiscoveryWorkJob] {
        let statement = try WorkflowSQLite.prepare(
            """
            SELECT \(Self.columns) FROM jobs
            WHERE kind IN ('feedDiscovery','newEpisodeNotification')
            ORDER BY id
            """,
            db: db
        )
        defer { sqlite3_finalize(statement) }
        return try readLegacyFeedDiscoveryRows(statement)
    }

    func readLegacyFeedDiscoveryRows(
        _ statement: OpaquePointer
    ) throws -> [LegacyFeedDiscoveryWorkJob] {
        var jobs: [LegacyFeedDiscoveryWorkJob] = []
        while sqlite3_step(statement) == SQLITE_ROW {
            guard let id = WorkflowSQLite.text(statement, 0).flatMap(UUID.init(uuidString:)),
                  let key = WorkflowSQLite.text(statement, 1),
                  let kind = WorkflowSQLite.text(statement, 2)
                    .flatMap(LegacyFeedDiscoveryJobKind.init(rawValue:)),
                  let subject = WorkflowSQLite.text(statement, 3).flatMap(UUID.init(uuidString:)),
                  let input = WorkflowSQLite.text(statement, 4),
                  let state = WorkflowSQLite.text(statement, 8)
                    .flatMap(WorkJobState.init(rawValue:)),
                  let resource = WorkflowSQLite.text(statement, 10)
                    .flatMap(WorkResourceClass.init(rawValue:)),
                  let notBefore = WorkflowSQLite.date(statement, 13),
                  let createdAt = WorkflowSQLite.date(statement, 23),
                  let updatedAt = WorkflowSQLite.date(statement, 24)
            else { throw JobStoreError.corruptRow }
            jobs.append(LegacyFeedDiscoveryWorkJob(
                id: id,
                idempotencyKey: key,
                kind: kind,
                subjectID: subject,
                inputVersion: input,
                occurrenceID: WorkflowSQLite.text(statement, 5),
                payloadVersion: Int(sqlite3_column_int64(statement, 6)),
                payload: WorkflowSQLite.data(statement, 7),
                state: state,
                priority: Int(sqlite3_column_int64(statement, 9)),
                resourceClass: resource,
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
                createdAt: createdAt,
                updatedAt: updatedAt
            ))
        }
        return jobs
    }

    func legacyFeedDiscoveryArtifacts(
        db: OpaquePointer
    ) throws -> [LegacyFeedDiscoveryArtifactRecord] {
        let statement = try WorkflowSQLite.prepare(
            """
            SELECT kind,subject_id,input_version,output_version,content_hash,
                   location,origin,schema_version,integrity,verified_at
            FROM artifacts
            WHERE kind IN ('feedDiscovery','notificationDelivery')
            ORDER BY kind,subject_id,input_version,output_version,id
            """,
            db: db
        )
        defer { sqlite3_finalize(statement) }
        var records: [LegacyFeedDiscoveryArtifactRecord] = []
        while sqlite3_step(statement) == SQLITE_ROW {
            guard let kind = WorkflowSQLite.text(statement, 0)
                    .flatMap(LegacyFeedDiscoveryArtifactKind.init(rawValue:)),
                  let subject = WorkflowSQLite.text(statement, 1)
                    .flatMap(UUID.init(uuidString:)),
                  let input = WorkflowSQLite.text(statement, 2),
                  let output = WorkflowSQLite.text(statement, 3),
                  let hash = WorkflowSQLite.text(statement, 4),
                  let integrity = WorkflowSQLite.text(statement, 8)
                    .flatMap(ArtifactIntegrity.init(rawValue:)),
                  let verified = WorkflowSQLite.date(statement, 9)
            else { throw JobStoreError.corruptRow }
            records.append(LegacyFeedDiscoveryArtifactRecord(
                kind: kind,
                subjectID: subject,
                inputVersion: input,
                outputVersion: output,
                contentHash: hash,
                location: WorkflowSQLite.text(statement, 5),
                origin: WorkflowSQLite.text(statement, 6),
                schemaVersion: Int(sqlite3_column_int64(statement, 7)),
                integrity: integrity,
                verifiedAt: verified
            ))
        }
        return records
    }

    func deleteLegacyFeedDiscoveryRows(_ db: OpaquePointer) throws {
        try WorkflowSQLite.execute(
            "DELETE FROM jobs WHERE kind IN ('feedDiscovery','newEpisodeNotification')",
            db
        )
        try WorkflowSQLite.execute(
            "DELETE FROM artifacts WHERE kind IN ('feedDiscovery','notificationDelivery')",
            db
        )
    }

    func insertRetirement(
        db: OpaquePointer,
        generation: UInt64,
        digest: String,
        jobCount: Int,
        artifactCount: Int
    ) throws {
        let statement = try WorkflowSQLite.prepare(
            """
            INSERT INTO legacy_feed_discovery_retirement(
              singleton,schema_version,source_generation,source_digest,
              retired_job_count,retired_artifact_count,completed_at
            ) VALUES(1,1,?,?,?,?,?)
            """,
            db: db
        )
        defer { sqlite3_finalize(statement) }
        try WorkflowSQLite.bind(Int64(generation), 1, statement, db)
        try WorkflowSQLite.bind(digest, 2, statement, db)
        try WorkflowSQLite.bind(Int64(jobCount), 3, statement, db)
        try WorkflowSQLite.bind(Int64(artifactCount), 4, statement, db)
        try WorkflowSQLite.bind(Date(), 5, statement, db)
        try WorkflowSQLite.stepDone(statement, db)
    }

    func retirementMatches(
        db: OpaquePointer,
        generation: UInt64,
        digest: String,
        jobCount: Int,
        artifactCount: Int
    ) throws -> Bool {
        let statement = try WorkflowSQLite.prepare(
            """
            SELECT source_generation,source_digest,retired_job_count,retired_artifact_count
            FROM legacy_feed_discovery_retirement WHERE singleton=1
            """,
            db: db
        )
        defer { sqlite3_finalize(statement) }
        guard sqlite3_step(statement) == SQLITE_ROW else { return false }
        return UInt64(sqlite3_column_int64(statement, 0)) == generation
            && WorkflowSQLite.text(statement, 1) == digest
            && sqlite3_column_int64(statement, 2) == Int64(jobCount)
            && sqlite3_column_int64(statement, 3) == Int64(artifactCount)
    }
}
