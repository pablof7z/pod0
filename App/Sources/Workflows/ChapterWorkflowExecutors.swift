import Foundation
import Pod0Core

@MainActor
final class PublisherChaptersJobExecutor: JobExecutor {
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
        let payload = try decodePayload(PublisherChaptersJobPayload.self, job: context.job)
        guard payload.sourceVersion == context.job.inputVersion else {
            return .failedPermanent(.init(
                classification: .invalidInput,
                message: "Publisher chapter payload version does not match its job."
            ))
        }
        guard let episode = store.episode(id: context.job.subjectID) else { return .obsolete }
        guard let sharedLibrary = store.sharedLibrary else {
            return .waitingForDependency(.init(
                classification: .missingDependency,
                message: "The shared chapter core is unavailable."
            ))
        }
        let expectedSelectionRevision = try sharedLibrary.authoritativeChapterReader
            .summary(episodeID: episode.id)?.selectionRevision ?? StateRevision(value: 0)

        let envelope = ChapterCapabilityRequestEnvelope(
            requestID: HostRequestId(uuid: UUID()),
            cancellationID: CancellationId(uuid: context.leaseToken),
            request: .publisher(.init(
                episodeID: EpisodeId(uuid: episode.id),
                podcastID: PodcastId(uuid: episode.podcastID),
                sourceURL: payload.url.absoluteString,
                generatedAt: UnixTimestampMilliseconds(date: Date()),
                durationMilliseconds: durationMilliseconds(episode.duration),
                deadlineAt: context.deadline.map(UnixTimestampMilliseconds.init(date:))
            ))
        )
        let response = await capability.execute(envelope)
        switch response.outcome {
        case .failed(let failure):
            return try publisherFailureOutcome(failure)
        case .observed(_, _, let qualification):
            do {
                let committed = try sharedLibrary.submitChapterObservation(
                    qualification,
                    commandID: CommandId(uuid: context.leaseToken),
                    cancellationID: CancellationId(uuid: context.leaseToken),
                    expectedSelectionRevision: expectedSelectionRevision
                )
                let receipt = try SharedChapterWorkflowReceipt(
                    summary: committed.snapshot.summary,
                    inputVersion: context.job.inputVersion,
                    publisherInputVersion: context.job.inputVersion
                )
                return .succeeded(outputVersion: try encodeReceipt(receipt))
            } catch {
                throw JobFailure.classify(error)
            }
        }
    }
}

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
        guard let episode = store.episode(id: context.job.subjectID) else { return .obsolete }
        guard let sharedLibrary = store.sharedLibrary else {
            return .waitingForDependency(.init(
                classification: .missingDependency,
                message: "The shared chapter core is unavailable."
            ))
        }
        guard let transcriptSummary = try sharedLibrary.authoritativeTranscriptReader.summary(
            episodeID: episode.id
        ), let transcript = sharedLibrary.authoritativeTranscriptReader.load(episodeID: episode.id)
        else {
            return .waitingForDependency(.init(
                classification: .missingDependency,
                message: "The selected transcript is unavailable."
            ))
        }

        let selected = try sharedLibrary.authoritativeChapterReader.selectedArtifactInput(
            episodeID: episode.id
        )
        let expectedSelectionRevision = selected?.selectionRevision ?? StateRevision(value: 0)
        let publisherArtifact: ChapterArtifactInput?
        if let selected {
            switch selected.artifact.provenance.source {
            case .publisher, .publisherEnriched:
                publisherArtifact = selected.artifact
            case .agentComposed:
                return .obsolete
            case .generated:
                publisherArtifact = nil
            case .unsupported:
                return .failedPermanent(.init(
                    classification: .unsupportedFormat,
                    message: "The selected chapter provenance is unsupported."
                ))
            }
        } else {
            publisherArtifact = nil
        }

        let prompt = ChapterModelPromptBuilder.make(
            episode: episode,
            transcript: transcript,
            publisherChapters: publisherArtifact?.chapters
        )
        let model = LLMModelReference(storedID: store.state.settings.chapterCompilationModel)
        let mode: ChapterModelObservationMode = publisherArtifact.map {
            .enrich(publisherArtifact: $0)
        } ?? .generate
        let envelope = ChapterCapabilityRequestEnvelope(
            requestID: HostRequestId(uuid: UUID()),
            cancellationID: CancellationId(uuid: context.leaseToken),
            request: .model(.init(
                episodeID: EpisodeId(uuid: episode.id),
                podcastID: PodcastId(uuid: episode.podcastID),
                formatVersion: 1,
                requestedTranscriptVersionID: transcriptSummary.transcriptVersionId,
                requestedTranscriptContentDigest: transcriptSummary.transcriptContentDigest,
                selectedTranscriptVersionID: transcriptSummary.transcriptVersionId,
                selectedTranscriptContentDigest: transcriptSummary.transcriptContentDigest,
                policyVersion: 1,
                provider: model.provider.rawValue,
                model: model.modelID,
                systemPrompt: prompt.system,
                userPrompt: prompt.user,
                generatedAt: UnixTimestampMilliseconds(date: Date()),
                durationMilliseconds: durationMilliseconds(episode.duration),
                mode: mode
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
                    expectedSelectionRevision: expectedSelectionRevision
                )
                let publisherVersion = publisherArtifact.flatMap { _ in
                    DesiredStatePlanner.publisherChapterInputVersion(episode)
                }
                let receipt = try SharedChapterWorkflowReceipt(
                    summary: committed.snapshot.summary,
                    inputVersion: context.job.inputVersion,
                    publisherInputVersion: publisherVersion
                )
                return .succeeded(outputVersion: try encodeReceipt(receipt))
            } catch {
                throw JobFailure.classify(error)
            }
        }
    }
}

private func durationMilliseconds(_ duration: TimeInterval?) -> UInt64? {
    guard let duration, duration.isFinite, duration >= 0 else { return nil }
    let milliseconds = duration * 1_000
    guard milliseconds <= Double(UInt64.max) else { return nil }
    return UInt64(milliseconds.rounded())
}

private func publisherFailureOutcome(
    _ failure: ChapterCapabilityFailure
) throws -> JobOutcome {
    switch failure.code {
    case .cancelled:
        return .cancelled
    case .coreUnavailable:
        return .waitingForDependency(.init(
            classification: .missingDependency,
            message: failure.safeDetail ?? "The shared chapter core is unavailable."
        ))
    case .invalidRequest, .invalidResponseMetadata, .responseTooLarge:
        return .failedPermanent(.init(
            classification: .invalidInput,
            message: failure.safeDetail ?? "The publisher chapter document is invalid."
        ))
    case .authentication:
        return .failedPermanent(.init(
            classification: .invalidInput,
            message: failure.safeDetail ?? "The publisher chapter document is unavailable."
        ))
    case .transport where failure.httpStatus == 404 || failure.httpStatus == 410:
        return .failedPermanent(.init(
            classification: .invalidInput,
            message: failure.safeDetail ?? "The publisher chapter document is unavailable."
        ))
    case .transport:
        throw JobFailure(
            classification: .network,
            message: failure.safeDetail ?? "Publisher chapter transport failed."
        )
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

private func decodePayload<T: Decodable>(_ type: T.Type, job: WorkJob) throws -> T {
    guard let payload = job.payload else {
        throw JobFailure(classification: .invalidInput, message: "Missing versioned job payload.")
    }
    do { return try JSONDecoder().decode(type, from: payload) }
    catch { throw JobFailure(classification: .invalidInput, message: error.localizedDescription) }
}
