import Foundation
import Pod0Core

/// Thin native capability adapter for the shared Rust recall workflow.
///
/// Rust owns evidence selection, embedding cache identity, index writes,
/// retrieval, ranking, recovery, cutover policy, and cancellation. This host
/// only executes bounded provider or exact-file requests and returns typed
/// observations.
struct CoreRecallHost: CoreRecallHosting {
    private let providers: any RecallProviderExecuting
    private let legacyIndexURL: URL?

    init(
        providers: any RecallProviderExecuting,
        legacyIndexURL: URL?
    ) {
        self.providers = providers
        self.legacyIndexURL = legacyIndexURL
    }

    func execute(_ request: HostRequest) async -> HostObservation {
        do {
            try Task.checkCancellation()
            return try await executeProviderRequest(request)
        } catch is CancellationError {
            return .cancelled
        } catch let error as CoreRecallHostError {
            return error.observation
        } catch let error as URLError {
            return Self.observation(for: error)
        } catch let error as EmbeddingsError {
            return Self.observation(for: error)
        } catch let error as RerankerError {
            return Self.observation(for: error)
        } catch is RecallProviderExecutionError {
            return .failed(code: .invalidResponse, safeDetail: "Unsupported recall provider")
        } catch {
            return .failed(code: .platformFailure, safeDetail: "Recall capability failed")
        }
    }

    private func executeProviderRequest(_ request: HostRequest) async throws -> HostObservation {
        switch request {
        case .embedRecallQuery(
            let queryID,
            let provider,
            let model,
            let text,
            let maximumDimensions
        ):
            let embedding = try await embed(
                provider: provider,
                model: model,
                texts: [text],
                dimensions: maximumDimensions
            ).only
            guard let embedding else { throw CoreRecallHostError.invalidResponse }
            return .recallQueryEmbedded(queryId: queryID, embedding: embedding)

        case .embedRecallSpans(
            let episodeID,
            let generationID,
            let provider,
            let model,
            let spans,
            let maximumDimensions
        ):
            guard !spans.isEmpty,
                  Set(spans.map(\.spanId)).count == spans.count else {
                throw CoreRecallHostError.invalidRequest
            }
            let embeddings = try await embed(
                provider: provider,
                model: model,
                texts: spans.map(\.text),
                dimensions: maximumDimensions
            )
            guard embeddings.count == spans.count else {
                throw CoreRecallHostError.invalidResponse
            }
            return .recallSpansEmbedded(
                episodeId: episodeID,
                generationId: generationID,
                embeddings: zip(spans, embeddings).map {
                    RecallSpanEmbeddingObservation(spanId: $0.spanId, embedding: $1)
                }
            )

        case .rerankRecallCandidates(
            let queryID,
            let provider,
            let model,
            let query,
            let candidates
        ):
            let order = try await providers.rerank(
                provider: provider,
                model: model,
                query: query,
                documents: candidates.map(\.excerpt)
            )
            try Task.checkCancellation()
            guard order.count == candidates.count,
                  Set(order) == Set(candidates.indices) else {
                throw CoreRecallHostError.invalidResponse
            }
            let rankings = try order.enumerated().map { index, candidateIndex in
                guard let rank = UInt16(exactly: index + 1) else {
                    throw CoreRecallHostError.invalidResponse
                }
                return RecallRerankObservation(
                    spanId: candidates[candidateIndex].spanId,
                    rank: rank
                )
            }
            return .recallCandidatesReranked(queryId: queryID, rankings: rankings)

        case .removeLegacyRecallIndexArtifacts:
            return .legacyRecallIndexArtifactsRemoved(
                removedFileCount: try removeLegacyIndexArtifacts()
            )

        default:
            throw CoreRecallHostError.invalidRequest
        }
    }

    /// Removes only the former disposable Swift execution index. Rust decides
    /// when this capability may run and commits ownership only after observing
    /// its typed result. Canonical transcript evidence is never stored here.
    private func removeLegacyIndexArtifacts() throws -> UInt8 {
        guard let legacyIndexURL else {
            throw CoreRecallHostError.legacyArtifactLocationUnavailable
        }
        let artifacts = [
            legacyIndexURL,
            URL(fileURLWithPath: legacyIndexURL.path + "-wal"),
            URL(fileURLWithPath: legacyIndexURL.path + "-shm"),
        ]
        let existing = try artifacts.compactMap { url -> URL? in
            do {
                let values = try url.resourceValues(forKeys: [
                    .isRegularFileKey,
                    .isSymbolicLinkKey,
                ])
                guard values.isRegularFile == true, values.isSymbolicLink != true else {
                    throw CoreRecallHostError.invalidLegacyArtifact
                }
                return url
            } catch let error as CocoaError where error.code == .fileNoSuchFile {
                return nil
            }
        }
        var removed: UInt8 = 0
        for url in existing {
            do {
                try FileManager.default.removeItem(at: url)
                removed += 1
            } catch let error as CocoaError where error.code == .fileNoSuchFile {
                continue
            }
        }
        return removed
    }

    private func embed(
        provider: RecallEmbeddingProvider,
        model: String,
        texts: [String],
        dimensions: UInt16
    ) async throws -> [RecallEmbeddingVector] {
        let vectors = try await providers.embed(
            provider: provider,
            model: model,
            dimensions: Int(dimensions),
            texts: texts
        )
        try Task.checkCancellation()
        guard vectors.count == texts.count,
              vectors.allSatisfy({ $0.count == Int(dimensions) }) else {
            throw CoreRecallHostError.invalidResponse
        }
        return try vectors.map { vector in
            RecallEmbeddingVector(values: try vector.map(Self.quantize))
        }
    }

    private static func quantize(_ value: Float) throws -> Int32 {
        let scaled = Double(value) * 1_000_000
        guard scaled.isFinite,
              scaled >= Double(Int32.min), scaled <= Double(Int32.max) else {
            throw CoreRecallHostError.invalidResponse
        }
        return Int32(scaled.rounded())
    }

    private static func observation(for error: URLError) -> HostObservation {
        switch error.code {
        case .cancelled:
            .cancelled
        case .timedOut:
            .failed(code: .timedOut, safeDetail: "Recall provider timed out")
        case .notConnectedToInternet, .networkConnectionLost, .dataNotAllowed:
            .failed(code: .offline, safeDetail: "Recall provider is offline")
        default:
            .failed(code: .providerUnavailable, safeDetail: "Recall provider unavailable")
        }
    }

    private static func observation(for error: EmbeddingsError) -> HostObservation {
        switch error {
        case .missingAPIKey, .providerMissingAPIKey:
            .failed(code: .providerUnavailable, safeDetail: "Recall provider is not configured")
        case .unauthorized, .providerUnauthorized:
            .failed(code: .unauthorized, safeDetail: "Recall provider authorization failed")
        case .decoding, .shapeMismatch, .providerDecoding, .dimensionMismatch:
            .failed(code: .invalidResponse, safeDetail: "Invalid recall provider response")
        case .rateLimited, .serverError, .transport,
             .providerRateLimited, .providerServerError, .providerTransport:
            .failed(code: .providerUnavailable, safeDetail: "Recall provider unavailable")
        }
    }

    private static func observation(for error: RerankerError) -> HostObservation {
        switch error {
        case .missingAPIKey:
            .failed(code: .providerUnavailable, safeDetail: "Recall provider is not configured")
        case .unauthorized:
            .failed(code: .unauthorized, safeDetail: "Recall provider authorization failed")
        case .decoding:
            .failed(code: .invalidResponse, safeDetail: "Invalid recall provider response")
        case .rateLimited, .serverError, .transport:
            .failed(code: .providerUnavailable, safeDetail: "Recall provider unavailable")
        }
    }
}

private enum CoreRecallHostError: Error {
    case invalidRequest
    case invalidResponse
    case invalidLegacyArtifact
    case legacyArtifactLocationUnavailable

    var observation: HostObservation {
        switch self {
        case .invalidRequest:
            .failed(code: .invalidResponse, safeDetail: "Invalid recall request")
        case .invalidResponse:
            .failed(code: .invalidResponse, safeDetail: "Invalid recall provider response")
        case .invalidLegacyArtifact:
            .failed(code: .invalidResponse, safeDetail: "Invalid legacy recall artifact")
        case .legacyArtifactLocationUnavailable:
            .failed(code: .platformFailure, safeDetail: "Legacy recall location unavailable")
        }
    }
}

private extension Collection {
    var only: Element? { count == 1 ? first : nil }
}
