import Foundation
import Pod0Core

/// Thin native capability adapter for the shared Rust recall workflow.
///
/// Rust owns evidence selection, embedding cache identity, index writes,
/// retrieval, ranking, recovery, cutover policy, and cancellation. This host
/// only executes bounded provider or exact-file requests and returns typed
/// observations.
struct CoreRecallHost: CoreRecallHosting {
    private let embedder: any EmbeddingsClient
    private let reranker: any RerankerClient
    private let legacyIndexURL: URL?
    private let isRerankingEnabled: @Sendable () async -> Bool

    init(
        embedder: any EmbeddingsClient,
        reranker: any RerankerClient,
        legacyIndexURL: URL?,
        isRerankingEnabled: @escaping @Sendable () async -> Bool
    ) {
        self.embedder = embedder
        self.reranker = reranker
        self.legacyIndexURL = legacyIndexURL
        self.isRerankingEnabled = isRerankingEnabled
    }

    func execute(_ request: HostRequest) async -> HostObservation {
        do {
            try Task.checkCancellation()
            return try await executeProviderRequest(request)
        } catch is CancellationError {
            return .cancelled
        } catch let error as CoreRecallHostError {
            return error.observation
        } catch is EmbeddingsError, is RerankerError {
            return .failed(code: .providerUnavailable, safeDetail: "Recall provider unavailable")
        } catch {
            return .failed(code: .platformFailure, safeDetail: "Recall capability failed")
        }
    }

    private func executeProviderRequest(_ request: HostRequest) async throws -> HostObservation {
        switch request {
        case .embedRecallQuery(let queryID, let text, let maximumDimensions):
            let embedding = try await embed(texts: [text], dimensions: maximumDimensions).only
            guard let embedding else { throw CoreRecallHostError.invalidResponse }
            return .recallQueryEmbedded(queryId: queryID, embedding: embedding)

        case .embedRecallSpans(
            let episodeID,
            let generationID,
            let spans,
            let maximumDimensions
        ):
            guard !spans.isEmpty,
                  Set(spans.map(\.spanId)).count == spans.count else {
                throw CoreRecallHostError.invalidRequest
            }
            let embeddings = try await embed(
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

        case .rerankRecallCandidates(let queryID, let query, let candidates):
            guard await isRerankingEnabled() else {
                return .failed(
                    code: .providerUnavailable,
                    safeDetail: "Recall reranking is disabled"
                )
            }
            let order = try await reranker.rerank(
                query: query,
                documents: candidates.map(\.excerpt),
                topN: candidates.count
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
        texts: [String],
        dimensions: UInt16
    ) async throws -> [RecallEmbeddingVector] {
        let vectors = try await embedder.embed(texts)
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
