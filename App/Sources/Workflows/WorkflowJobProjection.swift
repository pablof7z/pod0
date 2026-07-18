import Foundation

struct WorkflowJobKey: Hashable, Sendable {
    let kind: WorkJobKind
    let subjectID: UUID
}

enum WorkflowJobAction: String, CaseIterable, Sendable, Equatable {
    case retry
    case cancel
}

enum WorkflowJobActionResult: Sendable, Equatable {
    case accepted(WorkflowJobAction)
    case stale
    case notAllowed
    case alreadyComplete
    case notFound
    case failed
}

/// Coarse lifecycle state rendered by native screens. Lease ownership and
/// payload bytes stay inside the durable workflow implementation.
struct WorkflowJobProjection: Identifiable, Sendable, Equatable {
    let id: UUID
    let kind: WorkJobKind
    let subjectID: UUID
    let state: WorkJobState
    let resourceClass: WorkResourceClass
    let attempt: Int
    let maxAttempts: Int
    let notBefore: Date
    let externalProvider: String?
    let externalOperationState: String?
    let outputVersion: String?
    let lastErrorClass: JobErrorClass?
    let lastErrorMessage: String?
    let createdAt: Date
    let updatedAt: Date
    let allowedActions: Set<WorkflowJobAction>

    var key: WorkflowJobKey {
        WorkflowJobKey(kind: kind, subjectID: subjectID)
    }

    init(job: WorkJob) {
        id = job.id
        kind = job.kind
        subjectID = job.subjectID
        state = job.state
        resourceClass = job.resourceClass
        attempt = job.attempt
        maxAttempts = job.maxAttempts
        notBefore = job.notBefore
        externalProvider = job.externalProvider
        externalOperationState = job.externalOperationState
        outputVersion = job.outputVersion
        lastErrorClass = job.lastErrorClass
        lastErrorMessage = job.lastErrorMessage
        createdAt = job.createdAt
        updatedAt = job.updatedAt
        allowedActions = Self.actions(for: job.state, errorClass: job.lastErrorClass)
    }

    private static func actions(
        for state: WorkJobState,
        errorClass: JobErrorClass?
    ) -> Set<WorkflowJobAction> {
        switch state {
        case .pending, .leased, .running, .retryScheduled:
            return [.cancel]
        case .blocked:
            switch errorClass {
            case .unsafeToRetry, .invalidInput, .unsupportedFormat:
                return [.cancel]
            default:
                return [.retry, .cancel]
            }
        case .failedPermanent, .cancelled:
            switch errorClass {
            case .unsafeToRetry, .invalidInput, .unsupportedFormat:
                return []
            default:
                return [.retry]
            }
        case .obsolete, .succeeded:
            return []
        }
    }
}

/// A native screen's declared interest. Subject scopes include the latest
/// terminal row; attention scopes include only active or failed work.
struct WorkflowProjectionRequest: Hashable, Sendable {
    let subjectIDs: Set<UUID>
    let kinds: Set<WorkJobKind>
    let attentionKinds: Set<WorkJobKind>
    let recentKinds: Set<WorkJobKind>

    init(
        subjectIDs: some Sequence<UUID> = [],
        kinds: some Sequence<WorkJobKind> = [],
        attentionKinds: some Sequence<WorkJobKind> = [],
        recentKinds: some Sequence<WorkJobKind> = []
    ) {
        self.subjectIDs = Set(subjectIDs)
        self.kinds = Set(kinds)
        self.attentionKinds = Set(attentionKinds)
        self.recentKinds = Set(recentKinds)
    }

    var isEmpty: Bool {
        (subjectIDs.isEmpty || kinds.isEmpty) && attentionKinds.isEmpty && recentKinds.isEmpty
    }
}

struct WorkflowProjectionQuery: Sendable, Equatable {
    let subjectIDs: [UUID]
    let kinds: [WorkJobKind]
    let attentionKinds: [WorkJobKind]
    let recentKinds: [WorkJobKind]
    let limit: Int
}
