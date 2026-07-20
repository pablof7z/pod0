import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

final class CoreRecallIndexBoundaryTests: XCTestCase {
    func testSharedGoldenFixtureProducesTheSameRawCandidateContract() async throws {
        let fixture = try loadFixture()
        let embeddings = Dictionary(uniqueKeysWithValues: fixture.spans.map {
            ($0.text, $0.embedding.map { Float($0) / 1_000_000 })
        })
        let index = try VectorIndex(
            embedder: FixtureRecallEmbedder(embeddings: embeddings),
            inMemory: true,
            dimensions: fixture.dimensions
        )
        for episodeLow in [1, 2] as [UInt64] {
            let spans = fixture.spans.filter { $0.episodeLow == episodeLow }.map {
                CoreRecallIndexSpan(
                    spanID: EvidenceSpanId(high: 100, low: $0.spanLow),
                    generationID: EvidenceGenerationId(high: 200, low: $0.generationLow),
                    episodeID: EpisodeId(high: 300, low: $0.episodeLow),
                    podcastID: PodcastId(high: 400, low: $0.podcastLow),
                    text: $0.text
                )
            }
            let rebuilt = try await index.rebuildCoreRecallIndex(spans: spans)
            XCTAssertEqual(rebuilt, UInt32(spans.count))
        }

        let candidates = try await index.retrieveCoreRecallCandidates(
            queryVector: fixture.queryEmbedding.map { Float($0) / 1_000_000 },
            lexicalQuery: fixture.lexicalQuery,
            scope: .episode(episodeId: EpisodeId(high: 300, low: fixture.queryEpisodeLow)),
            maximumVectorCandidates: 3,
            maximumLexicalCandidates: 3,
            maximumTotalCandidates: 6
        )
        XCTAssertEqual(candidates.count, fixture.expected.count)
        for (candidate, expected) in zip(candidates, fixture.expected) {
            XCTAssertEqual(candidate.spanId, EvidenceSpanId(high: 100, low: expected.spanLow))
            XCTAssertEqual(candidate.vectorRank, expected.vectorRank)
            XCTAssertEqual(candidate.lexicalRank, expected.lexicalRank)
        }
    }

    private func loadFixture() throws -> RecallIndexFixture {
        let url = try XCTUnwrap(Bundle(for: Self.self).url(
            forResource: "recall-index-v1",
            withExtension: "json"
        ))
        return try JSONDecoder().decode(RecallIndexFixture.self, from: Data(contentsOf: url))
    }
}

private struct FixtureRecallEmbedder: EmbeddingsClient {
    let embeddings: [String: [Float]]

    func embed(_ texts: [String]) async throws -> [[Float]] {
        try texts.map { text in
            guard let embedding = embeddings[text] else {
                throw VectorStoreError.backingStorageFailure("Golden fixture embedding is missing")
            }
            return embedding
        }
    }
}

private struct RecallIndexFixture: Decodable {
    let dimensions: Int
    let lexicalQuery: String
    let queryEmbedding: [Int]
    let queryEpisodeLow: UInt64
    let spans: [RecallIndexFixtureSpan]
    let expected: [RecallIndexExpectedCandidate]

    enum CodingKeys: String, CodingKey {
        case dimensions, spans, expected
        case lexicalQuery = "lexical_query"
        case queryEmbedding = "query_embedding"
        case queryEpisodeLow = "query_episode_low"
    }
}

private struct RecallIndexFixtureSpan: Decodable {
    let spanLow: UInt64
    let generationLow: UInt64
    let episodeLow: UInt64
    let podcastLow: UInt64
    let text: String
    let embedding: [Int]

    enum CodingKeys: String, CodingKey {
        case text, embedding
        case spanLow = "span_low"
        case generationLow = "generation_low"
        case episodeLow = "episode_low"
        case podcastLow = "podcast_low"
    }
}

private struct RecallIndexExpectedCandidate: Decodable {
    let spanLow: UInt64
    let vectorRank: UInt16?
    let lexicalRank: UInt16?

    enum CodingKeys: String, CodingKey {
        case spanLow = "span_low"
        case vectorRank = "vector_rank"
        case lexicalRank = "lexical_rank"
    }
}
