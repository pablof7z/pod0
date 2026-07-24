import CSQLite3
import Foundation
@testable import Podcastr

enum LegacyFeedDiscoveryWorkflowTestSupport {
    static func encode<T: Encodable>(_ value: T) throws -> Data {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        return try encoder.encode(value)
    }

    static func makeJob(
        id: UUID = UUID(),
        kind: LegacyFeedDiscoveryJobKind,
        subjectID: UUID,
        inputVersion: String,
        occurrenceID: String,
        payload: Data,
        payloadVersion: Int = 1,
        state: WorkJobState = .pending,
        attempt: Int = 0,
        notBefore: Date = .distantPast,
        leaseToken: UUID? = nil,
        leaseExpiresAt: Date? = nil,
        externalOperationID: String? = nil,
        externalOperationState: String? = nil
    ) -> LegacyFeedDiscoveryWorkJob {
        LegacyFeedDiscoveryWorkJob(
            id: id,
            idempotencyKey: occurrenceID,
            kind: kind,
            subjectID: subjectID,
            inputVersion: inputVersion,
            occurrenceID: occurrenceID,
            payloadVersion: payloadVersion,
            payload: payload,
            state: state,
            priority: 0,
            resourceClass: kind == .feedDiscovery ? .planning : .notification,
            attempt: attempt,
            maxAttempts: 4,
            notBefore: notBefore,
            leaseToken: leaseToken,
            leaseOwner: leaseToken == nil ? nil : "legacy-owner",
            leaseExpiresAt: leaseToken == nil
                ? nil
                : (leaseExpiresAt ?? Date.distantFuture),
            externalProvider: nil,
            externalOperationID: externalOperationID,
            externalOperationState: externalOperationState,
            outputVersion: nil,
            lastErrorClass: nil,
            lastErrorMessage: nil,
            createdAt: Date(timeIntervalSince1970: 100),
            updatedAt: Date(timeIntervalSince1970: 100)
        )
    }

    static func insert(_ job: LegacyFeedDiscoveryWorkJob, into store: JobStore) throws {
        try store.withDatabase { db in
            let statement = try WorkflowSQLite.prepare(
                """
                INSERT INTO jobs(
                    id,idempotency_key,kind,subject_id,input_version,occurrence_id,
                    payload_version,payload,state,priority,resource_class,attempt,
                    max_attempts,not_before,lease_token,lease_owner,lease_expires_at,
                    external_provider,external_operation_id,external_operation_state,
                    output_version,last_error_class,last_error_message,created_at,updated_at
                ) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
                """,
                db: db
            )
            defer { sqlite3_finalize(statement) }
            try WorkflowSQLite.bind(job.id.uuidString, 1, statement, db)
            try WorkflowSQLite.bind(job.idempotencyKey, 2, statement, db)
            try WorkflowSQLite.bind(job.kind.rawValue, 3, statement, db)
            try WorkflowSQLite.bind(job.subjectID.uuidString, 4, statement, db)
            try WorkflowSQLite.bind(job.inputVersion, 5, statement, db)
            try WorkflowSQLite.bind(job.occurrenceID, 6, statement, db)
            try WorkflowSQLite.bind(Int64(job.payloadVersion), 7, statement, db)
            try WorkflowSQLite.bind(job.payload, 8, statement, db)
            try WorkflowSQLite.bind(job.state.rawValue, 9, statement, db)
            try WorkflowSQLite.bind(Int64(job.priority), 10, statement, db)
            try WorkflowSQLite.bind(job.resourceClass.rawValue, 11, statement, db)
            try WorkflowSQLite.bind(Int64(job.attempt), 12, statement, db)
            try WorkflowSQLite.bind(Int64(job.maxAttempts), 13, statement, db)
            try WorkflowSQLite.bind(job.notBefore, 14, statement, db)
            try WorkflowSQLite.bind(job.leaseToken?.uuidString, 15, statement, db)
            try WorkflowSQLite.bind(job.leaseOwner, 16, statement, db)
            try WorkflowSQLite.bind(job.leaseExpiresAt, 17, statement, db)
            try WorkflowSQLite.bind(job.externalProvider, 18, statement, db)
            try WorkflowSQLite.bind(job.externalOperationID, 19, statement, db)
            try WorkflowSQLite.bind(job.externalOperationState, 20, statement, db)
            try WorkflowSQLite.bind(job.outputVersion, 21, statement, db)
            try WorkflowSQLite.bind(job.lastErrorClass?.rawValue, 22, statement, db)
            try WorkflowSQLite.bind(job.lastErrorMessage, 23, statement, db)
            try WorkflowSQLite.bind(job.createdAt, 24, statement, db)
            try WorkflowSQLite.bind(job.updatedAt, 25, statement, db)
            try WorkflowSQLite.stepDone(statement, db)
        }
    }

    static func insert(
        _ artifact: LegacyFeedDiscoveryArtifactRecord,
        into store: JobStore
    ) throws {
        try store.withDatabase { db in
            try WorkflowSchemaMigrations.ensureArtifacts(db)
            let statement = try WorkflowSQLite.prepare(
                """
                INSERT INTO artifacts(
                    kind,subject_id,input_version,output_version,content_hash,
                    location,origin,schema_version,integrity,verified_at,selected
                ) VALUES(?,?,?,?,?,?,?,?,?,?,1)
                """,
                db: db
            )
            defer { sqlite3_finalize(statement) }
            try WorkflowSQLite.bind(artifact.kind.rawValue, 1, statement, db)
            try WorkflowSQLite.bind(artifact.subjectID.uuidString, 2, statement, db)
            try WorkflowSQLite.bind(artifact.inputVersion, 3, statement, db)
            try WorkflowSQLite.bind(artifact.outputVersion, 4, statement, db)
            try WorkflowSQLite.bind(artifact.contentHash, 5, statement, db)
            try WorkflowSQLite.bind(artifact.location, 6, statement, db)
            try WorkflowSQLite.bind(artifact.origin, 7, statement, db)
            try WorkflowSQLite.bind(Int64(artifact.schemaVersion), 8, statement, db)
            try WorkflowSQLite.bind(artifact.integrity.rawValue, 9, statement, db)
            try WorkflowSQLite.bind(artifact.verifiedAt, 10, statement, db)
            try WorkflowSQLite.stepDone(statement, db)
        }
    }
}
