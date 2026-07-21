import Foundation
import Pod0Core

extension WorkflowJobProjection {
    init(modelChapterWorkflow workflow: ModelChapterWorkflowProjection) {
        guard let episodeID = workflow.episodeId.uuid else {
            preconditionFailure("Rust model workflow returned an invalid episode ID")
        }
        id = OccurrenceIdentity.uuid(for: "rust-model-chapters:\(episodeID.uuidString)")
        kind = .chapterArtifacts
        subjectID = episodeID
        state = Self.state(for: workflow.stage)
        resourceClass = .utilityLLM
        attempt = Int(workflow.attempt)
        maxAttempts = Int(workflow.maxAttempts)
        notBefore = workflow.notBefore?.date ?? workflow.updatedAt.date
        externalProvider = Self.provider(in: workflow.configuredModel)
        externalOperationState = Self.operationState(for: workflow.stage)
        outputVersion = workflow.selectedArtifactId?.stableString
        lastErrorClass = workflow.failure.map {
            Self.errorClass($0.code, mayHaveSubmitted: $0.mayHaveSubmitted)
        }
        lastErrorMessage = workflow.failure?.safeDetail
        createdAt = workflow.createdAt.date
        updatedAt = workflow.updatedAt.date
        var actions: Set<WorkflowJobAction> = []
        if workflow.allowedActions.canRetry { actions.insert(.retry) }
        if workflow.allowedActions.canCancel { actions.insert(.cancel) }
        allowedActions = actions
        authority = .sharedRustModelChapters
        coreWorkflowRevision = workflow.workflowRevision.value
    }

    private static func state(for stage: ModelChapterWorkflowStage) -> WorkJobState {
        switch stage {
        case .awaitingTranscript, .awaitingPublisher: .pending
        case .preserved, .succeeded: .succeeded
        case .requested, .submissionAuthorized, .providerAccepted, .completionObserved: .running
        case .retryScheduled: .retryScheduled
        case .ambiguous, .blocked: .blocked
        case .failed, .unsupported: .failedPermanent
        case .cancelled: .cancelled
        }
    }

    private static func provider(in configuredModel: String) -> String? {
        let value = configuredModel.split(separator: ":", maxSplits: 1).first.map(String.init)
        return value?.isEmpty == false ? value : nil
    }

    private static func operationState(for stage: ModelChapterWorkflowStage) -> String {
        switch stage {
        case .awaitingTranscript: "awaitingTranscript"
        case .awaitingPublisher: "awaitingPublisher"
        case .preserved: "preserved"
        case .requested: "requested"
        case .submissionAuthorized: "submissionAuthorized"
        case .providerAccepted: "providerAccepted"
        case .ambiguous: "ambiguous"
        case .completionObserved: "completionObserved"
        case .retryScheduled: "retryScheduled"
        case .blocked: "blocked"
        case .failed: "failed"
        case .cancelled: "cancelled"
        case .succeeded: "succeeded"
        case .unsupported: "unsupported"
        }
    }

    private static func errorClass(
        _ code: ModelChapterWorkflowFailureCode,
        mayHaveSubmitted: Bool
    ) -> JobErrorClass {
        switch code {
        case .missingCredential: .missingCredential
        case .rateLimited: .rateLimited
        case .providerUnavailable, .timedOut, .transport: .network
        case .offline: .offline
        case .responseTooLarge: .unsupportedFormat
        case .staleTranscript, .stalePublisherBase, .selectionChanged, .storageUnavailable:
            .missingDependency
        case .ambiguousSubmission, .providerRecoveryUnavailable:
            .unsafeToRetry
        case .retryExhausted:
            mayHaveSubmitted ? .unsafeToRetry : .unexpected
        case .cancelled: .cancelled
        case .invalidRequest, .providerRejected, .invalidResponse, .qualificationRejected:
            .invalidInput
        case .unsupported: .unexpected
        }
    }
}
