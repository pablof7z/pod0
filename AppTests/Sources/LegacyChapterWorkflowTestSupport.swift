import CSQLite3
import Foundation
@testable import Podcastr

enum LegacyChapterWorkflowTestSupport {
    static func makeJob(
        id: UUID = UUID(),
        key: String = UUID().uuidString,
        kind: LegacyChapterWorkflowJobKind = .chapterArtifacts,
        episodeID: UUID = UUID(),
        inputVersion: String = "input-v1",
        occurrenceID: String? = nil,
        payloadVersion: Int = 1,
        payload: Data? = nil,
        state: WorkJobState = .pending,
        priority: Int = 0,
        resourceClass: WorkResourceClass = .utilityLLM,
        attempt: Int = 0,
        maxAttempts: Int = 8,
        notBefore: Date = .distantPast,
        leaseToken: UUID? = nil,
        leaseOwner: String? = nil,
        leaseExpiresAt: Date? = nil,
        externalProvider: String? = nil,
        externalOperationID: String? = nil,
        externalOperationState: String? = nil,
        outputVersion: String? = nil,
        lastErrorClass: JobErrorClass? = nil,
        lastErrorMessage: String? = nil,
        createdAt: Date = Date(timeIntervalSince1970: 100),
        updatedAt: Date = Date(timeIntervalSince1970: 100)
    ) -> LegacyChapterWorkflowJob {
        LegacyChapterWorkflowJob(
            id: id, idempotencyKey: key, kind: kind, subjectID: episodeID,
            inputVersion: inputVersion, occurrenceID: occurrenceID,
            payloadVersion: payloadVersion, payload: payload, state: state,
            priority: priority, resourceClass: resourceClass, attempt: attempt,
            maxAttempts: maxAttempts, notBefore: notBefore, leaseToken: leaseToken,
            leaseOwner: leaseOwner, leaseExpiresAt: leaseExpiresAt,
            externalProvider: externalProvider, externalOperationID: externalOperationID,
            externalOperationState: externalOperationState, outputVersion: outputVersion,
            lastErrorClass: lastErrorClass, lastErrorMessage: lastErrorMessage,
            createdAt: createdAt, updatedAt: updatedAt
        )
    }

    static func insert(_ job: LegacyChapterWorkflowJob, into store: JobStore) throws {
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

    static func remove(
        kind: LegacyChapterWorkflowJobKind,
        from store: JobStore
    ) throws {
        try store.withDatabase { db in
            let statement = try WorkflowSQLite.prepare(
                "DELETE FROM jobs WHERE kind=?",
                db: db
            )
            defer { sqlite3_finalize(statement) }
            try WorkflowSQLite.bind(kind.rawValue, 1, statement, db)
            try WorkflowSQLite.stepDone(statement, db)
        }
    }
}
