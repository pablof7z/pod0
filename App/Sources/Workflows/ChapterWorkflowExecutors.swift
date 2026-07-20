import Foundation
import Pod0Core

@MainActor
final class ChapterArtifactsJobExecutor: JobExecutor {
    private let store: AppStateStore
    private let capability: ChapterObservationCapabilityAdapter

    init(
        store: AppStateStore,
        capability: ChapterObservationCapabilityAdapter = .init()
    ) {
        self.store = store
        self.capability = capability
    }

    func run(_ context: JobAttemptContext) async throws -> JobOutcome {
        guard let sharedLibrary = store.sharedLibrary else {
            return .waitingForDependency(.init(
                classification: .missingDependency,
                message: "The shared chapter core is unavailable."
            ))
        }
        let plan = sharedLibrary.chapterModelPlan(
            episodeID: context.job.subjectID,
            configuredModel: store.state.settings.chapterCompilationModel
        )
        let request: PlannedChapterModelRequest
        switch plan {
        case .ready(let value): request = value
        case .episodeUnavailable, .staleTranscript, .preserveAgentComposed:
            return .obsolete
        case .transcriptUnavailable, .coreUnavailable:
            return .waitingForDependency(.init(
                classification: .missingDependency,
                message: "The selected transcript is unavailable."
            ))
        case .unsupportedArtifact:
            return .failedPermanent(.init(
                classification: .unsupportedFormat,
                message: "The selected chapter provenance is unsupported."
            ))
        case .invalidConfiguration, .invalidInput, .emptyTranscript, .inputTooLarge:
            return .failedPermanent(.init(
                classification: .invalidInput,
                message: "The shared chapter model request is invalid."
            ))
        }
        guard request.sourceVersion == context.job.inputVersion else { return .obsolete }
        let envelope = ChapterCapabilityRequestEnvelope(
            requestID: HostRequestId(uuid: UUID()),
            cancellationID: CancellationId(uuid: context.leaseToken),
            request: .model(.init(
                planned: request,
                generatedAt: UnixTimestampMilliseconds(date: Date())
            ))
        )
        let response = await capability.execute(envelope)
        switch response.outcome {
        case .failed(let failure):
            return try modelFailureOutcome(failure)
        case .observed(_, _, let qualification):
            do {
                let committed = try sharedLibrary.submitChapterObservation(
                    qualification,
                    commandID: CommandId(uuid: context.leaseToken),
                    cancellationID: CancellationId(uuid: context.leaseToken),
                    expectedSelectionRevision: request.expectedChapterSelectionRevision
                )
                let receipt = try SharedChapterWorkflowReceipt(
                    summary: committed.snapshot.summary,
                    inputVersion: context.job.inputVersion
                )
                return .succeeded(outputVersion: try encodeReceipt(receipt))
            } catch {
                throw JobFailure.classify(error)
            }
        }
    }
}

private func modelFailureOutcome(_ failure: ChapterCapabilityFailure) throws -> JobOutcome {
    switch failure.code {
    case .cancelled:
        return .cancelled
    case .authentication:
        return .blocked(reason: .init(
            classification: .missingCredential,
            message: failure.safeDetail ?? "Chapter model credentials are unavailable."
        ))
    case .coreUnavailable:
        return .waitingForDependency(.init(
            classification: .missingDependency,
            message: failure.safeDetail ?? "The shared chapter core is unavailable."
        ))
    case .invalidRequest:
        return .failedPermanent(.init(
            classification: .invalidInput,
            message: failure.safeDetail ?? "The chapter model request is invalid."
        ))
    case .transport, .responseTooLarge, .invalidResponseMetadata:
        throw JobFailure(
            classification: .transient,
            message: failure.safeDetail ?? "Chapter model observation failed."
        )
    }
}

private func encodeReceipt(_ receipt: SharedChapterWorkflowReceipt) throws -> String {
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.sortedKeys]
    return try encoder.encode(receipt).base64EncodedString()
}
