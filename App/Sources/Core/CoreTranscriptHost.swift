import Foundation
import Pod0Core

protocol CoreTranscriptHosting: Sendable {
    func execute(_ request: HostRequest) async -> HostObservation
}

/// Executes the exact transcript capability selected by Rust. Native owns
/// transient provider/OS handles only; Rust owns workflow, fallback, retry,
/// recovery, selection, and durable state.
actor CoreTranscriptHost: CoreTranscriptHosting {
    fileprivate static let localResponseLimit: UInt64 = 32 * 1_024 * 1_024
    private static let encoder: JSONEncoder = {
        let value = JSONEncoder()
        value.dateEncodingStrategy = .iso8601
        value.outputFormatting = [.sortedKeys]
        return value
    }()

    private let transport: any CoreTranscriptTransporting

    init(transport: any CoreTranscriptTransporting = LiveCoreTranscriptTransport()) {
        self.transport = transport
    }

    func execute(_ request: HostRequest) async -> HostObservation {
        guard case .executeTranscriptCapability(let capability) = request else {
            return .failed(
                code: .invalidResponse,
                safeDetail: "Non-transcript request sent to transcript host"
            )
        }
        guard case .accepted = validateTranscriptCapabilityRequest(request: capability) else {
            return wrappedFailure(.invalidRequest, detail: "Invalid transcript capability")
        }

        do {
            let raw = try await transport.execute(capability)
            try Task.checkCancellation()
            return try observation(raw, for: capability)
        } catch is CancellationError {
            return .transcriptCapabilityObserved(observation: .cancelled)
        } catch {
            return failure(error, for: capability)
        }
    }

    private func observation(
        _ raw: CoreTranscriptTransportObservation,
        for request: TranscriptCapabilityRequest
    ) throws -> HostObservation {
        let value: TranscriptCapabilityObservation
        switch raw {
        case .providerAccepted(let externalID, let status):
            value = .providerAccepted(
                externalOperationId: externalID,
                providerStatus: status
            )
        case .providerPending(let status, let retryAfter):
            value = .providerPending(
                providerStatus: status,
                retryAfterMilliseconds: retryAfter
            )
        case .completed(let transcript, let externalID, let status):
            let context = request.context
            guard transcript.episodeID == context.episodeId.uuid else {
                throw CoreTranscriptTransportError.invalidResponse
            }
            let payload = try Self.encoder.encode(transcript)
            guard UInt64(payload.count) <= request.maximumResponseBytes else {
                throw CoreTranscriptTransportError.responseTooLarge
            }
            let artifact = try TranscriptObservationMapper.map(
                transcript,
                context: TranscriptObservationContext(
                    podcastID: try context.podcastId.requiredUUID(),
                    sourceRevision: context.sourceRevision,
                    sourcePayloadDigest: TranscriptObservationMapper.payloadDigest(payload),
                    provider: TranscriptObservationMapper.defaultProvider(for: transcript.source)
                )
            )
            value = .completed(
                externalOperationId: externalID,
                providerStatus: status,
                artifact: artifact
            )
        }
        guard case .accepted = validateTranscriptCapabilityObservation(observation: value) else {
            throw CoreTranscriptTransportError.invalidResponse
        }
        return .transcriptCapabilityObserved(observation: value)
    }

    private func failure(
        _ error: Error,
        for request: TranscriptCapabilityRequest
    ) -> HostObservation {
        if let failure = error as? CoreTranscriptTransportError {
            return wrappedFailure(
                evidence(failure, request: request),
                detail: safeDetail(failure),
                retryAfterMilliseconds: failure.retryAfterMilliseconds
            )
        }
        if let failure = error as? AssemblyAITranscriptClient.TranscribeError {
            return wrappedFailure(evidence(failure, request: request), detail: failure.errorDescription)
        }
        if let failure = error as? ElevenLabsScribeClient.ScribeError {
            return wrappedFailure(evidence(failure, request: request), detail: failure.errorDescription)
        }
        if let failure = error as? OpenRouterWhisperClient.WhisperError {
            return wrappedFailure(evidence(failure, request: request), detail: failure.errorDescription)
        }
        if let failure = error as? AppleNativeSTTClient.STTError {
            return wrappedFailure(evidence(failure), detail: failure.errorDescription)
        }
        if error is TranscriptObservationMappingError {
            return wrappedFailure(.invalidResponse, detail: "Invalid transcript observation")
        }
        if let failure = error as? URLError {
            return wrappedFailure(evidence(failure, request: request), detail: "Transcript transport failed")
        }
        return wrappedFailure(phaseEvidence(.transport, request: request), detail: "Transcript capability failed")
    }

    private func wrappedFailure(
        _ evidence: TranscriptFailureEvidence,
        detail: String?,
        retryAfterMilliseconds: UInt64? = nil
    ) -> HostObservation {
        .transcriptCapabilityObserved(observation: .failed(
            evidence: evidence,
            safeDetail: detail.map { String($0.prefix(4_096)) },
            retryAfterMilliseconds: retryAfterMilliseconds
        ))
    }
}

private extension TranscriptCapabilityRequest {
    var context: TranscriptCapabilityContext {
        switch self {
        case .fetchPublisher(let context, _, _, _),
             .submitProvider(let context, _, _, _, _, _, _),
             .recoverProvider(let context, _, _, _, _, _, _, _),
             .transcribeLocal(let context, _, _, _): context
        }
    }

    var maximumResponseBytes: UInt64 {
        switch self {
        case .fetchPublisher(_, _, _, let value),
             .submitProvider(_, _, _, _, _, _, let value),
             .recoverProvider(_, _, _, _, _, _, _, let value): value
        case .transcribeLocal: CoreTranscriptHost.localResponseLimit
        }
    }
}

private extension PodcastId {
    func requiredUUID() throws -> UUID {
        guard let value = uuid else { throw CoreTranscriptTransportError.invalidRequest }
        return value
    }
}
