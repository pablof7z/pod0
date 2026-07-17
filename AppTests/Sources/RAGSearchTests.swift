import XCTest
@testable import Podcastr

final class RAGSearchTests: XCTestCase {

    func testSearchEmbedsQueryAndRanksMatchingChunks() async throws {
        let episodeID = UUID()
        let podcastID = UUID()
        let embedder = KeywordEmbeddingsClient()
        let store = try VectorIndex(embedder: embedder, inMemory: true, dimensions: 4)
        try await store.upsert(chunks: [
            Chunk(
                episodeID: episodeID,
                podcastID: podcastID,
                text: "Keto diet discussion about insulin sensitivity and appetite.",
                startMS: 10_000,
                endMS: 25_000
            ),
            Chunk(
                episodeID: UUID(),
                podcastID: podcastID,
                text: "A stamp collector talks about rare postal cancellations.",
                startMS: 0,
                endMS: 12_000
            ),
        ])

        let rag = RAGSearch(store: store, embedder: embedder, reranker: nil)
        let matches = try await rag.search(
            query: "keto insulin",
            scope: .podcast(podcastID),
            options: .init(k: 1, hybrid: true, rerank: false)
        )

        XCTAssertEqual(matches.count, 1)
        XCTAssertEqual(matches.first?.chunk.episodeID, episodeID)
        XCTAssertTrue(matches.first?.textHighlights.isEmpty == false)
    }

    func testEpisodesScopeFiltersToExplicitEpisodeSet() async throws {
        let selectedEpisodeID = UUID()
        let otherEpisodeID = UUID()
        let podcastID = UUID()
        let embedder = KeywordEmbeddingsClient()
        let store = try VectorIndex(embedder: embedder, inMemory: true, dimensions: 4)
        try await store.upsert(chunks: [
            Chunk(
                episodeID: selectedEpisodeID,
                podcastID: podcastID,
                text: "Keto insulin discussion from the selected episode.",
                startMS: 0,
                endMS: 10_000
            ),
            Chunk(
                episodeID: otherEpisodeID,
                podcastID: podcastID,
                text: "Keto insulin discussion from another episode.",
                startMS: 0,
                endMS: 10_000
            ),
        ])

        let rag = RAGSearch(store: store, embedder: embedder, reranker: nil)
        let matches = try await rag.search(
            query: "keto insulin",
            scope: .episodes([selectedEpisodeID]),
            options: .init(k: 5, hybrid: true, rerank: false)
        )

        XCTAssertEqual(matches.map(\.chunk.episodeID), [selectedEpisodeID])
    }

    func testVersionedGenerationIsInvisibleUntilSelectedAndReportsChunkCount() async throws {
        let episodeID = UUID()
        let podcastID = UUID()
        let embedder = KeywordEmbeddingsClient()
        let store = try VectorIndex(embedder: embedder, inMemory: true, dimensions: 4)
        let first = try await store.stageArtifact(
            chunks: [Chunk(
                episodeID: episodeID,
                podcastID: podcastID,
                text: "Keto insulin generation one",
                startMS: 0,
                endMS: 1_000
            )],
            episodeID: episodeID,
            generation: "generation-one",
            artifactKind: VectorIndex.semanticArtifactKind
        )
        XCTAssertEqual(first.chunkCount, 1)
        let verified = try await store.verifyArtifact(episodeID: episodeID, receipt: first)
        XCTAssertTrue(verified)

        let rag = RAGSearch(store: store, embedder: embedder, reranker: nil)
        let beforeFirstSelection = try await rag.search(
            query: "keto insulin", scope: .episode(episodeID),
            options: .init(k: 5, hybrid: true, rerank: false)
        )
        XCTAssertTrue(beforeFirstSelection.isEmpty)

        try await store.selectArtifact(episodeID: episodeID, receipt: first)
        let selectedFirst = try await store.selectedReceipt(
            episodeID: episodeID,
            artifactKind: VectorIndex.semanticArtifactKind
        )
        XCTAssertEqual(selectedFirst, first)

        let second = try await store.stageArtifact(
            chunks: [Chunk(
                episodeID: episodeID,
                podcastID: podcastID,
                text: "Stamp postal generation two",
                startMS: 0,
                endMS: 1_000
            )],
            episodeID: episodeID,
            generation: "generation-two",
            artifactKind: VectorIndex.semanticArtifactKind
        )
        let beforeSelection = try await rag.search(
            query: "keto insulin", scope: .episode(episodeID),
            options: .init(k: 5, hybrid: true, rerank: false)
        )
        XCTAssertEqual(beforeSelection.first?.chunk.text, "Keto insulin generation one")

        try await store.selectArtifact(episodeID: episodeID, receipt: second)
        let afterSelection = try await rag.search(
            query: "stamp postal", scope: .episode(episodeID),
            options: .init(k: 5, hybrid: true, rerank: false)
        )
        XCTAssertEqual(afterSelection.first?.chunk.text, "Stamp postal generation two")
    }

    func testSettingsAwareRerankerSkipsBaseClientWhenDisabled() async throws {
        let base = CountingReranker(order: [2, 1, 0])
        let reranker = SettingsAwareRerankerClient(base: base, isEnabled: { false })

        let order = try await reranker.rerank(
            query: "anything",
            documents: ["a", "b", "c"],
            topN: 2
        )

        let callCount = await base.callCount()
        XCTAssertEqual(order, [0, 1])
        XCTAssertEqual(callCount, 0)
    }

    func testSettingsAwareRerankerUsesBaseClientWhenEnabled() async throws {
        let base = CountingReranker(order: [2, 1, 0])
        let reranker = SettingsAwareRerankerClient(base: base, isEnabled: { true })

        let order = try await reranker.rerank(
            query: "anything",
            documents: ["a", "b", "c"],
            topN: 2
        )

        let callCount = await base.callCount()
        XCTAssertEqual(order, [2, 1, 0])
        XCTAssertEqual(callCount, 1)
    }
}

private struct KeywordEmbeddingsClient: EmbeddingsClient {
    private let keywords = ["keto", "insulin", "stamp", "postal"]

    func embed(_ texts: [String]) async throws -> [[Float]] {
        texts.map { text in
            let lower = text.lowercased()
            return keywords.map { lower.contains($0) ? 1 : 0 }
        }
    }
}

private actor CountingReranker: RerankerClient {
    private let order: [Int]
    private var calls = 0

    init(order: [Int]) {
        self.order = order
    }

    func rerank(query _: String, documents _: [String], topN _: Int?) async throws -> [Int] {
        calls += 1
        return order
    }

    func callCount() -> Int {
        calls
    }
}
