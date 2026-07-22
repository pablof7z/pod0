import Foundation

/// Decode-only compatibility values written before transcript workflow
/// authority moved to Rust. These values must never enter the live scheduler.
enum LegacyTranscriptWorkflowJobKind: String, Codable, Sendable, CaseIterable {
    case transcriptIngest
    case transcriptIndex
}

struct LegacyTranscriptWorkflowJob: Identifiable, Codable, Sendable, Equatable {
    let id: UUID
    let idempotencyKey: String
    let kind: LegacyTranscriptWorkflowJobKind
    let subjectID: UUID
    let inputVersion: String
    let occurrenceID: String?
    let payloadVersion: Int
    let payload: Data?
    let state: WorkJobState
    let priority: Int
    let resourceClass: WorkResourceClass
    let attempt: Int
    let maxAttempts: Int
    let notBefore: Date
    let leaseToken: UUID?
    let leaseOwner: String?
    let leaseExpiresAt: Date?
    let externalProvider: String?
    let externalOperationID: String?
    let externalOperationState: String?
    let outputVersion: String?
    let lastErrorClass: JobErrorClass?
    let lastErrorMessage: String?
    let createdAt: Date
    let updatedAt: Date
}
