import Pod0Core
import XCTest
@testable import Podcastr

final class CoreRecallHostTests: XCTestCase {
    func testProviderHostEmbedsQueriesAndStableSpanBatchesThenReranks() async throws {
        let embedder = CountingRecallEmbedder()
        let host = CoreRecallHost(
            providers: TestRecallProviderExecutor(
                embedder: embedder,
                reranker: ReverseRecallReranker()
            ),
            legacyIndexURL: temporaryLegacyIndexURL()
        )
        let queryID = RecallQueryId(high: 7, low: 8)
        let embedded = await host.execute(.embedRecallQuery(
            queryId: queryID,
            provider: .openRouter,
            model: "embedding-model",
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
            provider: .openRouter,
            model: "embedding-model",
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
            provider: .openRouter,
            model: "rerank-model",
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

    func testProviderFailureMalformedBatchAndUnsupportedRerankFailTyped() async {
        let host = CoreRecallHost(
            providers: TestRecallProviderExecutor(
                embedder: FailingRecallEmbedder(),
                reranker: ReverseRecallReranker()
            ),
            legacyIndexURL: temporaryLegacyIndexURL()
        )
        let provider = await host.execute(.embedRecallQuery(
            queryId: RecallQueryId(high: 1, low: 1),
            provider: .openRouter,
            model: "embedding-model",
            text: "private query",
            maximumDimensions: 3
        ))
        guard case .failed(code: .providerUnavailable, safeDetail: _) = provider else {
            return XCTFail("Expected content-free provider failure")
        }

        let malformed = await host.execute(.embedRecallSpans(
            episodeId: EpisodeId(high: 1, low: 2),
            generationId: EvidenceGenerationId(high: 1, low: 3),
            provider: .openRouter,
            model: "embedding-model",
            spans: [],
            maximumDimensions: 3
        ))
        guard case .failed(code: .invalidResponse, safeDetail: _) = malformed else {
            return XCTFail("Expected malformed batch to fail closed")
        }

        let rerank = await host.execute(.rerankRecallCandidates(
            queryId: RecallQueryId(high: 1, low: 4),
            provider: .unsupported(wireCode: 99),
            model: "rerank-model",
            query: "private query",
            candidates: [RecallRerankDocument(
                spanId: EvidenceSpanId(high: 1, low: 5),
                excerpt: "text"
            )]
        ))
        guard case .failed(code: .invalidResponse, safeDetail: _) = rerank else {
            return XCTFail("Expected unsupported provider to fail closed")
        }
    }

    func testProviderAuthorizationTimeoutAndCancellationMapDeterministically() async {
        let request = HostRequest.embedRecallQuery(
            queryId: RecallQueryId(high: 9, low: 1),
            provider: .openRouter,
            model: "embedding-model",
            text: "bounded text",
            maximumDimensions: 3
        )
        let unauthorized = await CoreRecallHost(
            providers: ErrorRecallProviderExecutor(mode: .unauthorized),
            legacyIndexURL: nil
        ).execute(request)
        guard case .failed(code: .unauthorized, safeDetail: _) = unauthorized else {
            return XCTFail("Expected typed authorization failure")
        }

        let timedOut = await CoreRecallHost(
            providers: ErrorRecallProviderExecutor(mode: .timedOut),
            legacyIndexURL: nil
        ).execute(request)
        guard case .failed(code: .timedOut, safeDetail: _) = timedOut else {
            return XCTFail("Expected typed timeout")
        }

        let cancelled = await CoreRecallHost(
            providers: ErrorRecallProviderExecutor(mode: .cancelled),
            legacyIndexURL: nil
        ).execute(request)
        XCTAssertEqual(cancelled, .cancelled)
    }

    func testDeferredProviderRequestWaitsForAttachmentAndCancelsWhileWaiting() async {
        let deferred = DeferredRecallHost()
        let pending = Task {
            await deferred.execute(.removeLegacyRecallIndexArtifacts)
        }
        await Task.yield()
        await deferred.attach(FixedRecallHost(
            observation: .legacyRecallIndexArtifactsRemoved(removedFileCount: 0)
        ))
        let attachedResult = await pending.value
        XCTAssertEqual(
            attachedResult,
            .legacyRecallIndexArtifactsRemoved(removedFileCount: 0)
        )

        let unattached = DeferredRecallHost()
        let cancelled = Task {
            await unattached.execute(.removeLegacyRecallIndexArtifacts)
        }
        await Task.yield()
        cancelled.cancel()
        let cancelledResult = await cancelled.value
        XCTAssertEqual(cancelledResult, .cancelled)
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

struct TestRecallProviderExecutor: RecallProviderExecuting {
    let embedder: any EmbeddingsClient
    let reranker: any RerankerClient

    func embed(
        provider: RecallEmbeddingProvider,
        model: String,
        dimensions: Int,
        texts: [String]
    ) async throws -> [[Float]] {
        guard case .unsupported = provider else {
            return try await embedder.embed(texts)
        }
        throw RecallProviderExecutionError.unsupportedProvider
    }

    func rerank(
        provider: RecallRerankProvider,
        model: String,
        query: String,
        documents: [String]
    ) async throws -> [Int] {
        if case .unsupported = provider {
            throw RecallProviderExecutionError.unsupportedProvider
        }
        return try await reranker.rerank(
            query: query,
            documents: documents,
            topN: documents.count
        )
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

private struct ErrorRecallProviderExecutor: RecallProviderExecuting {
    enum Mode: Sendable { case unauthorized, timedOut, cancelled }
    let mode: Mode

    func embed(
        provider: RecallEmbeddingProvider,
        model: String,
        dimensions: Int,
        texts: [String]
    ) async throws -> [[Float]] {
        switch mode {
        case .unauthorized: throw EmbeddingsError.unauthorized
        case .timedOut: throw URLError(.timedOut)
        case .cancelled: throw CancellationError()
        }
    }

    func rerank(
        provider: RecallRerankProvider,
        model: String,
        query: String,
        documents: [String]
    ) async throws -> [Int] {
        throw RecallProviderExecutionError.unsupportedProvider
    }
}

private struct FixedRecallHost: CoreRecallHosting {
    let observation: HostObservation

    func execute(_ request: HostRequest) async -> HostObservation { observation }
}
