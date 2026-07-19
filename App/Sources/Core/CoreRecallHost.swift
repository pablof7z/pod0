import Foundation
import Pod0Core

struct CoreRecallHost: CoreRecallHosting {
    private let projections: any CoreEvidenceProjectionProviding
    private let index: VectorIndex
    private let embedder: any EmbeddingsClient
    private let reranker: any RerankerClient
    private let isRerankingEnabled: @Sendable () async -> Bool

    init(
        projections: any CoreEvidenceProjectionProviding,
        index: VectorIndex,
        embedder: any EmbeddingsClient,
        reranker: any RerankerClient,
        isRerankingEnabled: @escaping @Sendable () async -> Bool
    ) {
        self.projections = projections
        self.index = index
        self.embedder = embedder
        self.reranker = reranker
        self.isRerankingEnabled = isRerankingEnabled
    }

    func execute(_ request: HostRequest) async -> HostObservation {
        do {
            try Task.checkCancellation()
            return try await executeRecall(request)
        } catch is CancellationError {
            return .cancelled
        } catch let error as CoreRecallHostError {
            return error.observation
        } catch is EmbeddingsError, is RerankerError {
            return .failed(code: .providerUnavailable, safeDetail: "Recall provider unavailable")
        } catch is VectorStoreError {
            return .failed(code: .indexUnavailable, safeDetail: "Recall index unavailable")
        } catch {
            return .failed(code: .platformFailure, safeDetail: "Recall capability failed")
        }
    }

    private func executeRecall(_ request: HostRequest) async throws -> HostObservation {
        switch request {
        case .embedRecallQuery(let queryID, let text, let maximumDimensions):
            let vectors = try await embedder.embed([text])
            try Task.checkCancellation()
            guard let vector = vectors.only,
                  !vector.isEmpty,
                  vector.count <= Int(maximumDimensions) else {
                throw CoreRecallHostError.invalidRequest
            }
            return .recallQueryEmbedded(
                queryId: queryID,
                embedding: RecallEmbeddingVector(values: try vector.map(Self.quantize))
            )
        case .retrieveRecallCandidates(
            let queryID,
            let scope,
            let lexicalQuery,
            let embedding,
            let maximumVectorCandidates,
            let maximumLexicalCandidates,
            let maximumTotalCandidates
        ):
            let candidates = try await index.retrieveCoreRecallCandidates(
                queryVector: embedding.values.map { Float($0) / 1_000_000 },
                lexicalQuery: lexicalQuery,
                scope: scope,
                maximumVectorCandidates: maximumVectorCandidates,
                maximumLexicalCandidates: maximumLexicalCandidates,
                maximumTotalCandidates: maximumTotalCandidates
            )
            try Task.checkCancellation()
            return .recallCandidatesRetrieved(queryId: queryID, candidates: candidates)
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
        case .rebuildRecallIndex(let episodeID, let generationID):
            let spans = try loadSelectedSpans(
                episodeID: episodeID,
                generationID: generationID
            )
            let count = try await index.rebuildCoreRecallIndex(spans: spans)
            try Task.checkCancellation()
            guard selectedGeneration(episodeID: episodeID) == generationID else {
                throw CoreRecallHostError.staleGeneration
            }
            return .recallIndexRebuilt(
                episodeId: episodeID,
                generationId: generationID,
                indexedSpanCount: count
            )
        default:
            throw CoreRecallHostError.invalidRequest
        }
    }

    private func loadSelectedSpans(
        episodeID: EpisodeId,
        generationID: EvidenceGenerationId
    ) throws -> [CoreRecallIndexSpan] {
        var offset: UInt32 = 0
        var expectedTotal: UInt32?
        var spans: [CoreRecallIndexSpan] = []
        var identities: Set<EvidenceSpanId> = []
        while true {
            guard let page = projections.evidenceIndexPage(
                episodeID: episodeID,
                offset: offset,
                maximumItems: 16
            ), page.stage == .ready,
            page.episodeId == episodeID,
            page.generationId == generationID,
            expectedTotal == nil || expectedTotal == page.totalSpans else {
                throw CoreRecallHostError.staleGeneration
            }
            expectedTotal = page.totalSpans
            guard !page.spans.isEmpty || !page.hasMore else {
                throw CoreRecallHostError.invalidResponse
            }
            for span in page.spans {
                guard span.episodeId == episodeID,
                      span.generationId == generationID,
                      identities.insert(span.spanId).inserted else {
                    throw CoreRecallHostError.invalidResponse
                }
                spans.append(CoreRecallIndexSpan(
                    spanID: span.spanId,
                    generationID: span.generationId,
                    episodeID: span.episodeId,
                    podcastID: span.podcastId,
                    text: span.text
                ))
            }
            guard page.hasMore else { break }
            guard let next = UInt32(exactly: spans.count), next > offset else {
                throw CoreRecallHostError.invalidResponse
            }
            offset = next
        }
        guard UInt32(exactly: spans.count) == expectedTotal, !spans.isEmpty else {
            throw CoreRecallHostError.invalidResponse
        }
        return spans
    }

    private func selectedGeneration(episodeID: EpisodeId) -> EvidenceGenerationId? {
        projections.evidenceIndexPage(
            episodeID: episodeID,
            offset: 0,
            maximumItems: 1
        )?.generationId
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
    case staleGeneration

    var observation: HostObservation {
        switch self {
        case .invalidRequest:
            .failed(code: .invalidResponse, safeDetail: "Invalid recall request")
        case .invalidResponse:
            .failed(code: .invalidResponse, safeDetail: "Invalid recall capability response")
        case .staleGeneration:
            .failed(code: .indexUnavailable, safeDetail: "Recall generation changed")
        }
    }
}

private extension Collection {
    var only: Element? { count == 1 ? first : nil }
}
