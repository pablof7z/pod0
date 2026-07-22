import Foundation

/// Decode-only compatibility schema for chapter jobs written by Swift builds
/// before Rust became authoritative. This type must never enter the normal
/// `JobStore` scheduler, projections, actions, or executor paths.
enum LegacyChapterWorkflowJobKind: String, Codable, Sendable {
    case publisherChapters
    case chapterArtifacts
}

struct LegacyChapterWorkflowJob: Identifiable, Codable, Sendable, Equatable {
    let id: UUID
    let idempotencyKey: String
    let kind: LegacyChapterWorkflowJobKind
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

/// Exact decoder for the final receipt emitted by the retired Swift model
/// chapter executor. The receipt is migration evidence, never live state.
struct LegacySharedChapterWorkflowReceiptV1: Codable, Sendable, Equatable {
    static let schemaVersion = 1

    let schemaVersion: Int
    let episodeID: UUID
    let inputVersion: String
    let artifactID: String
    let contentDigest: String
    let integrityDigest: String
    let selectionRevision: UInt64
}

enum LegacyChapterWorkflowSource {
    static func fingerprint(for jobs: [LegacyChapterWorkflowJob]) throws -> String {
        let source = try LegacyWorkflowBackupStorage.encodedData(
            jobs.sorted { $0.id.uuidString < $1.id.uuidString }
        )
        return ArtifactRepository.hash(source)
    }

    static func generation(for fingerprint: String) -> UInt64 {
        let raw = UInt64(fingerprint.prefix(16), radix: 16) ?? 0
        return max(1, raw & UInt64(Int64.max))
    }
}
