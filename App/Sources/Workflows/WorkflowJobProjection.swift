import Foundation
import Pod0Core

enum WorkflowProjectionAuthority: Sendable, Equatable {
    case swiftJobStore
    case sharedRustPublisherChapters
    case sharedRustModelChapters
    case sharedRustDownloads
    case sharedRustTranscripts
    case sharedRustScheduledAgents
}

extension WorkflowJobProjection {
    init(downloadWorkflow workflow: DownloadWorkflowProjection) {
        guard let episodeID = workflow.episodeId.uuid else {
            preconditionFailure("Rust download workflow returned an invalid episode ID")
        }
        id = OccurrenceIdentity.uuid(
            for: "rust-download:\(workflow.intentId.stableString)"
        )
        kind = .download
        subjectID = episodeID
        state = switch workflow.stage {
        case .waitingForEnvironment, .requested: .pending
        case .hostAccepted, .transferring, .staged, .removing: .running
        case .retryScheduled: .retryScheduled
        case .cancelled: .cancelled
        case .failed, .unsupported: .failedPermanent
        case .succeeded: .succeeded
        }
        resourceClass = .download
        attempt = Int(workflow.attempt)
        maxAttempts = max(8, Int(workflow.attempt))
        notBefore = workflow.notBefore?.date ?? workflow.updatedAt.date
        externalProvider = "native-download-host"
        externalOperationState = String(describing: workflow.stage)
        outputVersion = workflow.stage == .succeeded ? workflow.inputVersion : nil
        lastErrorClass = workflow.failure.map { Self.downloadErrorClass($0.code) }
        lastErrorMessage = workflow.failure?.safeDetail
        createdAt = workflow.updatedAt.date
        updatedAt = workflow.updatedAt.date
        var actions: Set<WorkflowJobAction> = []
        if workflow.allowedActions.canRetry { actions.insert(.retry) }
        if workflow.allowedActions.canCancel { actions.insert(.cancel) }
        allowedActions = actions
        authority = .sharedRustDownloads
        coreWorkflowRevision = workflow.workflowRevision.value
    }

    private static func downloadErrorClass(
        _ code: DownloadWorkflowFailureCode
    ) -> JobErrorClass {
        switch code {
        case .offline: .offline
        case .insufficientStorage, .wifiRequired, .storageUnavailable: .missingDependency
        case .missingEpisode, .invalidEnclosure, .invalidArtifact: .invalidInput
        case .timedOut, .transport, .hostRejected: .network
        case .permissionDenied: .missingCredential
        case .staleInput: .transient
        case .cancelled: .cancelled
        case .retryExhausted: .unexpected
        case .unsupported: .unsupportedFormat
        }
    }
}

/// Read-only semantic kinds rendered by native workflow surfaces. Chapter
/// kinds exist only here because their durable rows and actions are owned by
/// Rust; `WorkJobKind` remains limited to mutable Swift job-store records.
enum WorkflowProjectionKind: String, CaseIterable, Sendable {
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

    init(_ kind: WorkJobKind) {
        guard let projection = Self(rawValue: kind.rawValue) else {
            preconditionFailure("Every Swift job kind must have a projection kind")
        }
        self = projection
    }

    var swiftJobKind: WorkJobKind? {
        switch self {
        case .download, .transcriptIngest, .transcriptIndex, .autoDownload,
             .publisherChapters, .chapterArtifacts:
            nil
        default:
            WorkJobKind(rawValue: rawValue)
        }
    }
}

struct WorkflowJobKey: Hashable, Sendable {
    let kind: WorkflowProjectionKind
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
    let kind: WorkflowProjectionKind
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
    let authority: WorkflowProjectionAuthority
    let coreWorkflowRevision: UInt64?

    var key: WorkflowJobKey {
        WorkflowJobKey(kind: kind, subjectID: subjectID)
    }

    init(job: WorkJob) {
        id = job.id
        kind = WorkflowProjectionKind(job.kind)
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
        authority = .swiftJobStore
        coreWorkflowRevision = nil
    }

    init(publisherChapterWorkflow workflow: PublisherChapterWorkflowProjection) {
        guard let episodeID = workflow.episodeId.uuid else {
            preconditionFailure("Rust publisher workflow returned an invalid episode ID")
        }
        id = OccurrenceIdentity.uuid(
            for: "rust-publisher-chapters:\(episodeID.uuidString)"
        )
        kind = .publisherChapters
        subjectID = episodeID
        state = switch workflow.stage {
        case .requested: .running
        case .retryScheduled: .retryScheduled
        case .failed, .unsupported: .failedPermanent
        case .cancelled: .cancelled
        case .succeeded: .succeeded
        }
        resourceClass = .planning
        attempt = Int(workflow.attempt)
        maxAttempts = Int(workflow.maxAttempts)
        notBefore = workflow.notBefore?.date ?? workflow.updatedAt.date
        externalProvider = nil
        externalOperationState = nil
        outputVersion = workflow.selectedArtifactId?.stableString
        lastErrorClass = workflow.failure.map { Self.errorClass($0.code) }
        lastErrorMessage = workflow.failure?.safeDetail
        createdAt = workflow.createdAt.date
        updatedAt = workflow.updatedAt.date
        var actions: Set<WorkflowJobAction> = []
        if workflow.canRetry { actions.insert(.retry) }
        if workflow.canCancel { actions.insert(.cancel) }
        allowedActions = actions
        authority = .sharedRustPublisherChapters
        coreWorkflowRevision = workflow.workflowRevision.value
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

    private static func errorClass(
        _ code: PublisherChapterWorkflowFailureCode
    ) -> JobErrorClass {
        switch code {
        case .offline: .offline
        case .timedOut, .transport: .network
        case .notFound, .invalidResponse, .invalidDocument: .invalidInput
        case .responseTooLarge: .unsupportedFormat
        case .selectionChanged: .transient
        case .storageUnavailable: .missingDependency
        case .unsupported: .unexpected
        }
    }
}

/// A native screen's declared interest. Subject scopes include the latest
/// terminal row; attention scopes include only active or failed work.
struct WorkflowProjectionRequest: Hashable, Sendable {
    let subjectIDs: Set<UUID>
    let kinds: Set<WorkflowProjectionKind>
    let attentionKinds: Set<WorkflowProjectionKind>
    let recentKinds: Set<WorkflowProjectionKind>

    init(
        subjectIDs: some Sequence<UUID> = [],
        kinds: some Sequence<WorkflowProjectionKind> = [],
        attentionKinds: some Sequence<WorkflowProjectionKind> = [],
        recentKinds: some Sequence<WorkflowProjectionKind> = []
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
    let kinds: [WorkflowProjectionKind]
    let attentionKinds: [WorkflowProjectionKind]
    let recentKinds: [WorkflowProjectionKind]
    let limit: Int
}
