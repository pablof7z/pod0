import Foundation

enum WorkJobKind: String, CaseIterable, Codable, Sendable {
    case feedDiscovery
    case download
    case transcriptIngest
    case transcriptIndex
    case publisherChapters
    case chapterArtifacts
    case metadataIndex
    case autoDownload
    case newEpisodeNotification
    case scheduledAgentRun
}

enum WorkResourceClass: String, CaseIterable, Codable, Sendable {
    case planning
    case download
    case onDeviceSTT
    case remoteSTT
    case embedding
    case utilityLLM
    case scheduledAgent
    case notification
}

enum WorkJobState: String, CaseIterable, Codable, Sendable {
    case pending
    case leased
    case running
    case retryScheduled
    case blocked
    case failedPermanent
    case cancelled
    case obsolete
    case succeeded

    var isActive: Bool {
        switch self {
        case .pending, .leased, .running, .retryScheduled, .blocked: true
        case .failedPermanent, .cancelled, .obsolete, .succeeded: false
        }
    }
}

enum JobErrorClass: String, CaseIterable, Codable, Sendable {
    case transient
    case rateLimited
    case missingCredential
    case missingDependency
    case unsafeToRetry
    case invalidInput
    case cancelled
    case unexpected
}

struct JobFailure: Error, Codable, Sendable, Equatable {
    let classification: JobErrorClass
    let message: String
}

struct DesiredJob: Sendable, Equatable {
    let idempotencyKey: String
    let kind: WorkJobKind
    let subjectID: UUID
    let inputVersion: String
    let occurrenceID: String?
    let payloadVersion: Int
    let payload: Data?
    let priority: Int
    let resourceClass: WorkResourceClass
    let maxAttempts: Int

    init(
        idempotencyKey: String,
        kind: WorkJobKind,
        subjectID: UUID,
        inputVersion: String,
        occurrenceID: String? = nil,
        payloadVersion: Int = 1,
        payload: Data? = nil,
        priority: Int = 0,
        resourceClass: WorkResourceClass,
        maxAttempts: Int = 8
    ) {
        precondition(!idempotencyKey.isEmpty)
        self.idempotencyKey = idempotencyKey
        self.kind = kind
        self.subjectID = subjectID
        self.inputVersion = inputVersion
        self.occurrenceID = occurrenceID
        self.payloadVersion = payloadVersion
        self.payload = payload
        self.priority = priority
        self.resourceClass = resourceClass
        self.maxAttempts = max(1, maxAttempts)
    }
}

struct WorkJob: Identifiable, Sendable, Equatable {
    let id: UUID
    let idempotencyKey: String
    let kind: WorkJobKind
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

struct JobAttemptContext: Sendable {
    let job: WorkJob
    let leaseToken: UUID
    let deadline: Date?
}

enum JobOutcome: Sendable, Equatable {
    case succeeded(outputVersion: String?)
    case retry(notBefore: Date, error: JobFailure)
    case blocked(reason: JobFailure)
    case waitingForDependency(JobFailure)
    case obsolete
    case cancelled
    case failedPermanent(JobFailure)
}

protocol JobExecutor: Sendable {
    func run(_ context: JobAttemptContext) async throws -> JobOutcome
}

enum JobStoreError: LocalizedError {
    case sqlite(String)
    case transitionRejected
    case corruptRow
    case unsupportedSchema(component: String, detail: String)

    var errorDescription: String? {
        switch self {
        case .sqlite(let message): "Workflow database failed: \(message)"
        case .transitionRejected: "The job attempt no longer owns the active lease."
        case .corruptRow: "A workflow job row is invalid."
        case .unsupportedSchema(let component, let detail):
            "The \(component) workflow schema is unsupported: \(detail)."
        }
    }
}
