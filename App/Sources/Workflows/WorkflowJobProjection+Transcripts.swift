import Foundation
import Pod0Core

extension WorkflowJobProjection {
    init(transcriptWorkflow workflow: TranscriptWorkflowProjection) {
        guard let episodeID = workflow.episodeId.uuid else {
            preconditionFailure("Rust transcript workflow returned an invalid episode ID")
        }
        id = OccurrenceIdentity.uuid(for: "rust-transcript:\(episodeID.uuidString)")
        kind = workflow.stage.projectionKind
        subjectID = episodeID
        state = workflow.stage.jobState
        resourceClass = kind == .transcriptIndex
            ? .embedding
            : workflow.provider == .appleSpeech ? .onDeviceSTT : .remoteSTT
        attempt = Int(workflow.attempt)
        maxAttempts = max(8, Int(workflow.attempt))
        notBefore = workflow.notBefore?.date ?? workflow.updatedAt.date
        externalProvider = workflow.provider.displayCode
        externalOperationState = workflow.stage.displayCode
        outputVersion = workflow.stage == .succeeded ? workflow.sourceRevision : nil
        lastErrorClass = workflow.failure.map { $0.code.jobErrorClass }
        lastErrorMessage = workflow.failure?.safeDetail
        createdAt = workflow.updatedAt.date
        updatedAt = workflow.updatedAt.date
        var actions: Set<WorkflowJobAction> = []
        if workflow.allowedActions.canRetry { actions.insert(.retry) }
        if workflow.allowedActions.canCancel { actions.insert(.cancel) }
        allowedActions = actions
        authority = .sharedRustTranscripts
        coreWorkflowRevision = workflow.workflowRevision.value
    }
}

private extension TranscriptWorkflowStage {
    var projectionKind: WorkflowProjectionKind {
        switch self {
        case .transcriptCommitted, .evidenceRequested, .succeeded:
            .transcriptIndex
        default:
            .transcriptIngest
        }
    }

    var jobState: WorkJobState {
        switch self {
        case .awaitingPrerequisite, .blocked: .blocked
        case .requested, .publisherRequested, .submissionAuthorized,
             .providerAccepted, .completionObserved, .transcriptCommitted,
             .evidenceRequested: .running
        case .retryScheduled: .retryScheduled
        case .failed, .unsupported: .failedPermanent
        case .cancelled: .cancelled
        case .succeeded: .succeeded
        }
    }

    var displayCode: String {
        switch self {
        case .awaitingPrerequisite: "awaitingPrerequisite"
        case .requested: "requested"
        case .publisherRequested: "publisherRequested"
        case .submissionAuthorized: "submissionAuthorized"
        case .providerAccepted: "providerAccepted"
        case .completionObserved: "completionObserved"
        case .transcriptCommitted: "transcriptCommitted"
        case .evidenceRequested: "evidenceRequested"
        case .retryScheduled: "retryScheduled"
        case .blocked: "blocked"
        case .failed: "failed"
        case .cancelled: "cancelled"
        case .succeeded: "succeeded"
        case .unsupported: "unsupported"
        }
    }
}

private extension TranscriptProvider {
    var displayCode: String {
        switch self {
        case .assemblyAi: "assemblyAI"
        case .elevenLabsScribe: "elevenLabsScribe"
        case .openRouterWhisper: "openRouterWhisper"
        case .appleSpeech: "appleSpeech"
        case .unsupported: "unsupported"
        }
    }
}

private extension TranscriptWorkflowFailureCode {
    var jobErrorClass: JobErrorClass {
        switch self {
        case .missingCredential: .missingCredential
        case .missingLocalAudio, .storageUnavailable: .missingDependency
        case .offline: .offline
        case .rateLimited: .rateLimited
        case .timedOut, .transport, .providerUnavailable: .network
        case .ambiguousSubmission, .providerRecoveryUnavailable: .unsafeToRetry
        case .responseTooLarge, .unsupportedProvider: .unsupportedFormat
        case .cancelled: .cancelled
        case .staleInput: .transient
        case .invalidRequest, .permissionDenied, .providerRejected, .invalidResponse:
            .invalidInput
        case .publisherUnavailable, .retryExhausted, .unsupported:
            .unexpected
        }
    }
}
