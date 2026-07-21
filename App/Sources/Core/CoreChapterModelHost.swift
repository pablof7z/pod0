import Foundation
import Pod0Core

protocol CoreChapterModelHosting: Sendable {
    func execute(_ request: HostRequest) async -> HostObservation
}

/// Executes the exact provider request selected by Rust. It resolves native
/// credentials and decodes only the provider envelope; chapter JSON remains
/// opaque until the durable Rust workflow qualifies it.
actor CoreChapterModelHost: CoreChapterModelHosting {
    private let transport: any CoreChapterModelTransporting

    init(transport: any CoreChapterModelTransporting = LiveChapterModelTransport()) {
        self.transport = transport
    }

    func execute(_ request: HostRequest) async -> HostObservation {
        switch request {
        case .executeChapterModel(
            let episodeID,
            let generation,
            let submissionFenceID,
            let execution
        ):
            let result = await transport.execute(execution)
            switch result {
            case .success(let response):
                return .chapterModelCompleted(
                    episodeId: episodeID,
                    generation: generation,
                    submissionFenceId: submissionFenceID,
                    completion: ChapterModelCompletionObservation(
                        completion: response.completion,
                        provider: response.provider,
                        model: response.model,
                        promptTokens: response.usage.flatMap { Self.unsigned($0.promptTokens) },
                        completionTokens: response.usage.flatMap {
                            Self.unsigned($0.completionTokens)
                        },
                        cachedTokens: response.usage.flatMap { Self.unsigned($0.cachedTokens) },
                        reasoningTokens: response.usage.flatMap {
                            Self.unsigned($0.reasoningTokens)
                        },
                        costMicrousd: response.usage.flatMap { Self.microusd($0.costUSD) },
                        providerOperationId: nil,
                        providerStatus: "completed",
                        providerGeneratedAt: nil
                    )
                )
            case .failure(let failure):
                return Self.failed(
                    episodeID: episodeID,
                    generation: generation,
                    submissionFenceID: submissionFenceID,
                    failure: failure
                )
            }
        case .recoverChapterModelOperation(
            let episodeID,
            let generation,
            let submissionFenceID,
            _,
            _,
            _,
            _,
            _
        ):
            // Current OpenRouter/Ollama transports are synchronous and expose
            // no safe lookup API. Recovery must never duplicate the POST.
            return .chapterModelFailed(
                episodeId: episodeID,
                generation: generation,
                submissionFenceId: submissionFenceID,
                code: .providerRecoveryUnavailable,
                safeDetail: "Provider operation recovery is unavailable",
                retryAfterMilliseconds: nil
            )
        default:
            return .failed(
                code: .invalidResponse,
                safeDetail: "Non-model request sent to chapter model host"
            )
        }
    }

    private static func failed(
        episodeID: EpisodeId,
        generation: UInt64,
        submissionFenceID: ChapterModelSubmissionFenceId,
        failure: ChapterCapabilityFailure
    ) -> HostObservation {
        let code: ChapterModelHostFailureCode
        if let status = failure.httpStatus {
            code = .httpResponse(statusCode: status)
        } else {
            code = switch failure.code {
            case .invalidRequest: .invalidRequest
            case .authentication: .missingCredential
            case .cancelled: .cancelled
            case .responseTooLarge: .responseTooLarge
            case .invalidResponseMetadata: .invalidResponse
            case .transport, .coreUnavailable: .transport
            }
        }
        return .chapterModelFailed(
            episodeId: episodeID,
            generation: generation,
            submissionFenceId: submissionFenceID,
            code: code,
            safeDetail: failure.safeDetail,
            retryAfterMilliseconds: failure.retryAfterMilliseconds
        )
    }

    private static func unsigned(_ value: Int) -> UInt64? {
        guard value >= 0 else { return nil }
        return UInt64(value)
    }

    private static func microusd(_ value: Double?) -> UInt64? {
        guard let value, value.isFinite, value >= 0 else { return nil }
        let scaled = value * 1_000_000
        guard scaled <= Double(UInt64.max) else { return nil }
        return UInt64(scaled.rounded())
    }
}
