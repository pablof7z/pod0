import Pod0Core
import XCTest
@testable import Podcastr

final class CoreRecallHostTests: XCTestCase {
    func testProviderHostEmbedsQueriesAndStableSpanBatchesThenReranks() async throws {
        let embedder = CountingRecallEmbedder()
        let host = CoreRecallHost(
            embedder: embedder,
            reranker: ReverseRecallReranker(),
            legacyIndexURL: temporaryLegacyIndexURL(),
            isRerankingEnabled: { true }
        )
        let queryID = RecallQueryId(high: 7, low: 8)
        let embedded = await host.execute(.embedRecallQuery(
            queryId: queryID,
            text: "memory",
            maximumDimensions: 3
        ))
        XCTAssertEqual(embedded, .recallQueryEmbedded(
            queryId: queryID,
            embedding: RecallEmbeddingVector(values: [1_000_000, 0, 0])
        ))

        let episodeID = EpisodeId(high: 10, low: 11)
        let generationID = EvidenceGenerationId(high: 12, low: 13)
        let spans = [
            RecallEmbeddingInput(
                spanId: EvidenceSpanId(high: 14, low: 15),
                text: "A durable memory model connects exact evidence."
            ),
            RecallEmbeddingInput(
                spanId: EvidenceSpanId(high: 16, low: 17),
                text: "Queue policy remains owned by the shared kernel."
            ),
        ]
        let batch = await host.execute(.embedRecallSpans(
            episodeId: episodeID,
            generationId: generationID,
            spans: spans,
            maximumDimensions: 3
        ))
        XCTAssertEqual(batch, .recallSpansEmbedded(
            episodeId: episodeID,
            generationId: generationID,
            embeddings: [
                RecallSpanEmbeddingObservation(
                    spanId: spans[0].spanId,
                    embedding: RecallEmbeddingVector(values: [1_000_000, 0, 0])
                ),
                RecallSpanEmbeddingObservation(
                    spanId: spans[1].spanId,
                    embedding: RecallEmbeddingVector(values: [0, 1_000_000, 0])
                ),
            ]
        ))

        let reranked = await host.execute(.rerankRecallCandidates(
            queryId: queryID,
            query: "memory",
            candidates: spans.map {
                RecallRerankDocument(spanId: $0.spanId, excerpt: $0.text)
            }
        ))
        XCTAssertEqual(reranked, .recallCandidatesReranked(
            queryId: queryID,
            rankings: [
                RecallRerankObservation(spanId: spans[1].spanId, rank: 1),
                RecallRerankObservation(spanId: spans[0].spanId, rank: 2),
            ]
        ))
        let callCount = await embedder.callCount
        XCTAssertEqual(callCount, 2)
    }

    func testProviderFailureMalformedBatchAndDisabledRerankFailTyped() async {
        let host = CoreRecallHost(
            embedder: FailingRecallEmbedder(),
            reranker: ReverseRecallReranker(),
            legacyIndexURL: temporaryLegacyIndexURL(),
            isRerankingEnabled: { false }
        )
        let provider = await host.execute(.embedRecallQuery(
            queryId: RecallQueryId(high: 1, low: 1),
            text: "private query",
            maximumDimensions: 3
        ))
        guard case .failed(code: .providerUnavailable, safeDetail: _) = provider else {
            return XCTFail("Expected content-free provider failure")
        }

        let malformed = await host.execute(.embedRecallSpans(
            episodeId: EpisodeId(high: 1, low: 2),
            generationId: EvidenceGenerationId(high: 1, low: 3),
            spans: [],
            maximumDimensions: 3
        ))
        guard case .failed(code: .invalidResponse, safeDetail: _) = malformed else {
            return XCTFail("Expected malformed batch to fail closed")
        }

        let rerank = await host.execute(.rerankRecallCandidates(
            queryId: RecallQueryId(high: 1, low: 4),
            query: "private query",
            candidates: [RecallRerankDocument(
                spanId: EvidenceSpanId(high: 1, low: 5),
                excerpt: "text"
            )]
        ))
        guard case .failed(code: .providerUnavailable, safeDetail: _) = rerank else {
            return XCTFail("Expected disabled reranker fallback signal")
        }
    }
}

private func temporaryLegacyIndexURL() -> URL {
    FileManager.default.temporaryDirectory
        .appendingPathComponent("pod0-recall-host-\(UUID().uuidString)", isDirectory: true)
        .appendingPathComponent("vectors.sqlite")
}

private actor CountingRecallEmbedder: EmbeddingsClient {
    private(set) var callCount = 0

    func embed(_ texts: [String]) async throws -> [[Float]] {
        callCount += 1
        return texts.map { text in
            text.localizedCaseInsensitiveContains("memory") ? [1, 0, 0] : [0, 1, 0]
        }
    }
}

private struct FailingRecallEmbedder: EmbeddingsClient {
    func embed(_ texts: [String]) async throws -> [[Float]] {
        throw EmbeddingsError.rateLimited
    }
}

private struct ReverseRecallReranker: RerankerClient {
    func rerank(query: String, documents: [String], topN: Int?) async throws -> [Int] {
        Array(documents.indices.reversed())
    }
}
