import Foundation
import Pod0Core

enum LegacyTranscriptWorkflowDispositionMapper {
    static func map(
        _ selection: LegacyTranscriptWorkflowSelection,
        configuration: TranscriptWorkflowConfiguration
    ) -> LegacyTranscriptWorkflowCutoverDisposition {
        let job = selection.job
        if let evidence = selection.evidenceInputVersion {
            return .indexPending(evidenceInputVersion: evidence)
        }
        if selection.selectedTranscriptExists {
            return .succeeded(attempt: boundedOptionalAttempt(job.attempt))
        }
        if let external = externalDisposition(job, configuration: configuration) {
            return external
        }
        switch job.state {
        case .pending:
            return .restart(attempt: nextAttempt(job.attempt))
        case .leased, .running:
            return .restart(attempt: currentAttempt(job.attempt))
        case .retryScheduled:
            return .restart(attempt: nextAttempt(job.attempt))
        case .blocked:
            if job.lastErrorClass == .unsafeToRetry {
                return .ambiguous(attempt: currentAttempt(job.attempt))
            }
            return .blocked(
                attempt: boundedOptionalAttempt(job.attempt),
                failureCode: failureCode(job.lastErrorClass),
                failureDetail: boundedDetail(job.lastErrorMessage),
                mayHaveSubmitted: mayHaveSubmitted(job)
            )
        case .failedPermanent:
            return .failed(
                attempt: boundedOptionalAttempt(job.attempt),
                failureCode: failureCode(job.lastErrorClass),
                failureDetail: boundedDetail(job.lastErrorMessage),
                mayHaveSubmitted: mayHaveSubmitted(job)
            )
        case .cancelled:
            return .cancelled(
                attempt: boundedOptionalAttempt(job.attempt),
                mayHaveSubmitted: mayHaveSubmitted(job)
            )
        case .succeeded, .obsolete:
            return .ambiguous(attempt: currentAttempt(job.attempt))
        }
    }

    private static func externalDisposition(
        _ job: LegacyTranscriptWorkflowJob,
        configuration: TranscriptWorkflowConfiguration
    ) -> LegacyTranscriptWorkflowCutoverDisposition? {
        guard let provider = job.externalProvider else { return nil }
        if provider == "publisherTranscript" {
            return .restart(attempt: nextAttempt(job.attempt))
        }
        guard let externalID = job.externalOperationID, !externalID.isBlank else {
            return .ambiguous(attempt: currentAttempt(job.attempt))
        }
        let recoverable = switch (provider, configuration.provider) {
        case ("assemblyAI", .assemblyAi), ("elevenLabsScribe", .elevenLabsScribe): true
        default: false
        }
        guard recoverable else {
            return .ambiguous(attempt: currentAttempt(job.attempt))
        }
        return .recoverProvider(
            attempt: currentAttempt(job.attempt),
            externalOperationId: externalID,
            providerStatus: boundedStatus(job.externalOperationState)
        )
    }

    private static func mayHaveSubmitted(_ job: LegacyTranscriptWorkflowJob) -> Bool {
        if job.externalProvider != nil || job.externalOperationID != nil { return true }
        guard job.attempt > 0 else { return false }
        switch job.lastErrorClass {
        case .missingCredential, .missingDependency, .unsupportedFormat, .invalidInput:
            return false
        default:
            return true
        }
    }

    private static func failureCode(_ value: JobErrorClass?) -> String {
        switch value {
        case .missingCredential: "missing_credential"
        case .missingDependency: "missing_local_audio"
        case .rateLimited: "rate_limited"
        case .offline: "offline"
        case .network, .transient: "transport"
        case .unsupportedFormat: "unsupported_provider"
        case .unsafeToRetry: "ambiguous_submission"
        case .corruptArtifact: "invalid_response"
        case .invalidInput: "invalid_request"
        case .cancelled: "cancelled"
        case .unexpected, nil: "retry_exhausted"
        }
    }

    private static func currentAttempt(_ value: Int) -> UInt16 {
        UInt16(clamping: max(1, min(value, 8)))
    }

    private static func nextAttempt(_ value: Int) -> UInt16 {
        UInt16(clamping: max(1, min(value + 1, 8)))
    }

    private static func boundedOptionalAttempt(_ value: Int) -> UInt16? {
        value > 0 ? currentAttempt(value) : nil
    }

    private static func boundedDetail(_ value: String?) -> String? {
        value.map { String($0.prefix(16_384)) }
    }

    private static func boundedStatus(_ value: String?) -> String? {
        value.map { String($0.prefix(1_024)) }
    }
}

extension LegacyTranscriptWorkflowCutoverDisposition {
    var classification: LegacyTranscriptWorkflowBackupClassification {
        switch self {
        case .restart: .restart
        case .recoverProvider: .recoverProvider
        case .ambiguous: .ambiguous
        case .blocked: .blocked
        case .failed: .failed
        case .cancelled: .cancelled
        case .succeeded: .succeeded
        case .indexPending: .indexPending
        case .indexSucceeded: .indexSucceeded
        }
    }
}
