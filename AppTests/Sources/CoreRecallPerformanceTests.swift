import Pod0Core
import XCTest
@testable import Podcastr

final class CoreRecallPerformanceTests: XCTestCase {
    /// Five thousand spans approximate an actively prepared library of roughly
    /// eighty hour-long episodes at one semantic span per minute.
    func testRepresentativeHybridCandidateRetrievalMeetsLocalBudget() async throws {
        let index = try VectorIndex(
            embedder: PerformanceRecallEmbedder(),
            inMemory: true,
            dimensions: 3
        )
        let podcastID = PodcastId(high: 41, low: 1)
        for episodeOffset in 0..<50 {
            let episodeNumber = UInt64(episodeOffset + 1)
            let episodeID = EpisodeId(high: 42, low: episodeNumber)
            let generationID = EvidenceGenerationId(high: 43, low: episodeNumber)
            let spans = (0..<100).map { spanOffset in
                let isNeedle = spanOffset == 50
                return CoreRecallIndexSpan(
                    spanID: EvidenceSpanId(
                        high: episodeNumber,
                        low: UInt64(spanOffset + 1)
                    ),
                    generationID: generationID,
                    episodeID: episodeID,
                    podcastID: podcastID,
                    text: isNeedle
                        ? "needle evidence for durable podcast recall"
                        : "background discussion number \(spanOffset) in episode \(episodeOffset)"
                )
            }
            let rebuiltCount = try await index.rebuildCoreRecallIndex(spans: spans)
            XCTAssertEqual(rebuiltCount, 100)
        }

        let query = [Float](arrayLiteral: 1, 0, 0)
        _ = try await retrieve(index: index, query: query)
        var samples: [Double] = []
        for _ in 0..<20 {
            let start = ContinuousClock.now
            let candidates = try await retrieve(index: index, query: query)
            samples.append(milliseconds(ContinuousClock.now - start))
            XCTAssertFalse(candidates.isEmpty)
            XCTAssertLessThanOrEqual(candidates.count, 40)
        }

        samples.sort()
        let p95 = samples[18]
        XCTAssertLessThan(
            p95,
            100,
            "5,000-span vector+lexical retrieval p95 was \(p95) ms"
        )
    }

    private func retrieve(
        index: VectorIndex,
        query: [Float]
    ) async throws -> [RecallCandidateObservation] {
        try await index.retrieveCoreRecallCandidates(
            queryVector: query,
            lexicalQuery: "needle evidence",
            scope: .library,
            maximumVectorCandidates: 20,
            maximumLexicalCandidates: 20,
            maximumTotalCandidates: 40
        )
    }

    private func milliseconds(_ duration: Duration) -> Double {
        let parts = duration.components
        return Double(parts.seconds) * 1_000
            + Double(parts.attoseconds) / 1_000_000_000_000_000
    }
}

private struct PerformanceRecallEmbedder: EmbeddingsClient {
    func embed(_ texts: [String]) async throws -> [[Float]] {
        texts.map {
            $0.contains("needle") ? [1, 0, 0] : [0, 1, 0]
        }
    }
}
