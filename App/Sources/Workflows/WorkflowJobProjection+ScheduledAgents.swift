import Foundation
import Pod0Core

extension WorkflowJobProjection {
    init(scheduledAgentWorkflow workflow: ScheduledAgentWorkflowProjection) {
        guard let taskID = workflow.taskId.uuid else {
            preconditionFailure("Rust scheduled workflow returned an invalid task ID")
        }
        id = OccurrenceIdentity.uuid(
            for: "rust-scheduled-agent:\(workflow.occurrenceId.high):\(workflow.occurrenceId.low)"
        )
        kind = .scheduledAgentRun
        subjectID = taskID
        state = switch workflow.stage {
        case .pending: .pending
        case .requested: .leased
        case .hostAccepted: .running
        case .retryScheduled: .retryScheduled
        case .blocked, .ambiguous: .blocked
        case .cancelled: .cancelled
        case .obsolete: .obsolete
        case .failedPermanent, .unsupported: .failedPermanent
        case .succeeded: .succeeded
        }
        resourceClass = .scheduledAgent
        attempt = Int(workflow.attempt)
        maxAttempts = 12
        notBefore = workflow.notBefore?.date ?? workflow.updatedAt.date
        externalProvider = "native-agent-host"
        externalOperationState = String(describing: workflow.stage)
        outputVersion = workflow.outputDigest?.stableString
        lastErrorClass = workflow.failure.map { Self.scheduledAgentErrorClass($0.code) }
        lastErrorMessage = workflow.failure?.safeDetail
        createdAt = workflow.updatedAt.date
        updatedAt = workflow.updatedAt.date
        var actions: Set<WorkflowJobAction> = []
        if workflow.allowedActions.canRetry { actions.insert(.retry) }
        if workflow.allowedActions.canCancel { actions.insert(.cancel) }
        allowedActions = actions
        authority = .sharedRustScheduledAgents
        coreWorkflowRevision = workflow.workflowRevision.value
    }

    private static func scheduledAgentErrorClass(
        _ code: ScheduledAgentFailureCode
    ) -> JobErrorClass {
        switch code {
        case .missingCredential: .missingCredential
        case .offline: .offline
        case .network: .network
        case .rateLimited: .rateLimited
        case .providerUnavailable, .storageUnavailable: .missingDependency
        case .permissionDenied: .missingCredential
        case .invalidOutput: .invalidInput
        case .unsafeToRetry: .unsafeToRetry
        case .cancelled: .cancelled
        case .unexpected, .retryExhausted: .unexpected
        case .unsupported: .unsupportedFormat
        }
    }
}
