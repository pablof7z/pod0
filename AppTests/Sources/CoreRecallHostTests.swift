import Pod0Core
import XCTest
@testable import Podcastr

final class CoreRecallHostTests: XCTestCase {
    func testRebuildIsIdempotentAndRecallCapabilitiesPreserveExactCoreIdentity() async throws {
        let embedder = CountingRecallEmbedder()
        let index = try VectorIndex(embedder: embedder, inMemory: true, dimensions: 3)
        let fixture = RecallProjectionFixture()
        let host = CoreRecallHost(
            projections: fixture,
            index: index,
            embedder: embedder,
            reranker: ReverseRecallReranker(),
            isRerankingEnabled: { true }
        )
        let rebuild = HostRequest.rebuildRecallIndex(
            episodeId: fixture.episodeID,
            generationId: fixture.generationID
        )

        let first = await host.execute(rebuild)
        let second = await host.execute(rebuild)

        XCTAssertEqual(first, .recallIndexRebuilt(
            episodeId: fixture.episodeID,
            generationId: fixture.generationID,
            indexedSpanCount: 2
        ))
        XCTAssertEqual(second, first)
        let embeddingCallCount = await embedder.callCount
        XCTAssertEqual(embeddingCallCount, 1)

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

        let retrieved = await host.execute(.retrieveRecallCandidates(
            queryId: queryID,
            scope: .episode(episodeId: fixture.episodeID),
            lexicalQuery: "memory",
            embedding: RecallEmbeddingVector(values: [1_000_000, 0, 0]),
            maximumVectorCandidates: 2,
            maximumLexicalCandidates: 2,
            maximumTotalCandidates: 4
        ))
        guard case .recallCandidatesRetrieved(_, let candidates) = retrieved else {
            return XCTFail("Expected typed raw candidates")
        }
        XCTAssertEqual(Set(candidates.map(\.generationId)), [fixture.generationID])
        XCTAssertEqual(Set(candidates.map(\.spanId)), Set(fixture.spans.map(\.spanId)))
        let memory = try XCTUnwrap(candidates.first { $0.spanId == fixture.spans[0].spanId })
        XCTAssertEqual(memory.vectorRank, 1)
        XCTAssertEqual(memory.lexicalRank, 1)

        let reranked = await host.execute(.rerankRecallCandidates(
            queryId: queryID,
            query: "memory",
            candidates: fixture.spans.map {
                RecallRerankDocument(spanId: $0.spanId, excerpt: $0.text)
            }
        ))
        XCTAssertEqual(reranked, .recallCandidatesReranked(
            queryId: queryID,
            rankings: [
                RecallRerankObservation(spanId: fixture.spans[1].spanId, rank: 1),
                RecallRerankObservation(spanId: fixture.spans[0].spanId, rank: 2),
            ]
        ))
    }

    func testProviderFailureMalformedGenerationAndDisabledRerankFailTyped() async throws {
        let failing = FailingRecallEmbedder()
        let index = try VectorIndex(embedder: failing, inMemory: true, dimensions: 3)
        let fixture = RecallProjectionFixture()
        let host = CoreRecallHost(
            projections: fixture,
            index: index,
            embedder: failing,
            reranker: ReverseRecallReranker(),
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

        let rerank = await host.execute(.rerankRecallCandidates(
            queryId: RecallQueryId(high: 1, low: 2),
            query: "private query",
            candidates: [RecallRerankDocument(spanId: fixture.spans[0].spanId, excerpt: "text")]
        ))
        guard case .failed(code: .providerUnavailable, safeDetail: _) = rerank else {
            return XCTFail("Expected disabled reranker fallback signal")
        }

        let stale = await host.execute(.rebuildRecallIndex(
            episodeId: fixture.episodeID,
            generationId: EvidenceGenerationId(high: 99, low: 99)
        ))
        guard case .failed(code: .indexUnavailable, safeDetail: _) = stale else {
            return XCTFail("Expected stale generation to fail closed")
        }
    }
}

private struct RecallProjectionFixture: CoreEvidenceProjectionProviding {
    let episodeID = EpisodeId(high: 10, low: 11)
    let podcastID = PodcastId(high: 12, low: 13)
    let generationID = EvidenceGenerationId(high: 14, low: 15)
    let spans: [EvidenceIndexSpanProjection]

    init() {
        spans = [
            EvidenceIndexSpanProjection(
                spanId: EvidenceSpanId(high: 20, low: 21),
                generationId: generationID,
                episodeId: episodeID,
                podcastId: podcastID,
                text: "A durable memory model connects exact evidence."
            ),
            EvidenceIndexSpanProjection(
                spanId: EvidenceSpanId(high: 22, low: 23),
                generationId: generationID,
                episodeId: episodeID,
                podcastId: podcastID,
                text: "Queue policy remains owned by the shared kernel."
            ),
        ]
    }

    func evidenceIndexPage(
        episodeID: EpisodeId,
        offset: UInt32,
        maximumItems: UInt16
    ) -> EvidenceIndexProjection? {
        let start = Int(offset)
        let end = min(spans.count, start + Int(maximumItems))
        let page = start < spans.count ? Array(spans[start..<end]) : []
        return EvidenceIndexProjection(
            episodeId: episodeID,
            stage: .ready,
            generationId: generationID,
            transcriptContentDigest: ContentDigest(word0: 1, word1: 2, word2: 3, word3: 4),
            spans: page,
            totalSpans: UInt32(spans.count),
            hasMore: end < spans.count
        )
    }
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
