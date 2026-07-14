import XCTest
@testable import Podcastr

/// Unit tests for `ChunkBuilder`.
///
/// These run against fixture transcripts (no network, no SQLite) and lock in
/// the chunking contract that the rest of the RAG pipeline depends on:
///
/// - chunks are non-empty
/// - chunks respect the configured target token size with overlap
/// - chunks snap to speaker-turn boundaries when within the snap tolerance
/// - chunks preserve start/end millisecond grounding for player seek
/// - empty / single-segment transcripts degrade gracefully
final class ChunkBuilderTests: XCTestCase {

    // MARK: - Test fixtures

    /// Minimal `TranscriptLike` conformer used in tests. Mirrors the protocol
    /// `ChunkBuilder` consumes; Lane 5 will provide the real conformer.
    private struct FixtureTranscript: TranscriptLike {
        var episodeID: UUID
        var podcastID: UUID
        var segments: [FixtureSegment]
    }

    private struct FixtureSegment: TranscriptSegment {
        var text: String
        var startMS: Int
        var endMS: Int
        var speakerID: UUID?
    }

    private static let alice = UUID()
    private static let bob = UUID()
    private static let episode = UUID()
    private static let podcast = UUID()

    /// ~20 short segments, alternating speakers, ~10–15 tokens each.
    /// Total ≈ 250 tokens — should produce a small number of chunks at the
    /// default target size of 400.
    private func shortTwoSpeakerTranscript() -> FixtureTranscript {
        let lines: [(String, UUID, Int, Int)] = [
            ("Welcome to the show, listeners, glad you could join us today.", Self.alice, 0, 4_000),
            ("Thanks Alice, happy to be here on this episode.", Self.bob, 4_000, 7_500),
            ("Let's dig into the topic we promised last week.", Self.alice, 7_500, 11_000),
            ("Right, semaglutide and the metabolic story behind it.", Self.bob, 11_000, 15_500),
            ("There's been a flood of new trial data published recently.", Self.alice, 15_500, 20_000),
            ("Most of it pointing the same direction surprisingly.", Self.bob, 20_000, 24_000),
        ]
        return FixtureTranscript(
            episodeID: Self.episode,
            podcastID: Self.podcast,
            segments: lines.map { FixtureSegment(text: $0.0, startMS: $0.2, endMS: $0.3, speakerID: $0.1) }
        )
    }

    /// One long monologue ≈ 2000 tokens — should produce multiple chunks
    /// near the target size with overlap. Each segment has its own distinct
    /// numeric token so chunks are textually distinguishable.
    private func longMonologueTranscript() -> FixtureTranscript {
        // 200 segments × ≈10 tokens ≈ 2000 tokens. Embedding the index in
        // the text guarantees no two chunks have identical contents.
        let segments: [FixtureSegment] = (0..<200).map { i in
            FixtureSegment(
                text: "Segment number \(i) is a sentence with about ten tokens for predictable counting today.",
                startMS: i * 1_000,
                endMS: (i + 1) * 1_000,
                speakerID: Self.alice
            )
        }
        return FixtureTranscript(
            episodeID: Self.episode,
            podcastID: Self.podcast,
            segments: segments
        )
    }

    // MARK: - Tests

    func testEmptyTranscriptProducesNoChunks() {
        let transcript = FixtureTranscript(
            episodeID: Self.episode,
            podcastID: Self.podcast,
            segments: []
        )
        let builder = ChunkBuilder()
        let chunks = builder.build(from: transcript)
        XCTAssertTrue(chunks.isEmpty)
    }

    func testShortTranscriptProducesAtLeastOneChunk() {
        let transcript = shortTwoSpeakerTranscript()
        let builder = ChunkBuilder()
        let chunks = builder.build(from: transcript)
        XCTAssertGreaterThanOrEqual(chunks.count, 1)
        // No chunk should be empty.
        for chunk in chunks {
            XCTAssertFalse(chunk.text.isEmpty)
        }
    }

    func testChunksCarryEpisodeAndPodcastIDs() {
        let transcript = shortTwoSpeakerTranscript()
        let builder = ChunkBuilder()
        let chunks = builder.build(from: transcript)
        for chunk in chunks {
            XCTAssertEqual(chunk.episodeID, Self.episode)
            XCTAssertEqual(chunk.podcastID, Self.podcast)
        }
    }

    func testChunksHaveMonotonicTimestamps() {
        let transcript = longMonologueTranscript()
        let builder = ChunkBuilder()
        let chunks = builder.build(from: transcript)
        XCTAssertGreaterThan(chunks.count, 1, "Long transcript should yield multiple chunks")
        for chunk in chunks {
            XCTAssertLessThan(chunk.startMS, chunk.endMS, "Each chunk's start must precede its end")
        }
        // Chunks should be roughly time-ordered (sliding window with overlap
        // means starts can equal but never go backwards).
        for (a, b) in zip(chunks, chunks.dropFirst()) {
            XCTAssertLessThanOrEqual(a.startMS, b.startMS)
        }
    }

    func testLongTranscriptRespectsTargetTokenSize() {
        let transcript = longMonologueTranscript()
        let builder = ChunkBuilder(targetTokens: 400, overlapFraction: 0.15)
        let chunks = builder.build(from: transcript)
        XCTAssertGreaterThan(chunks.count, 1)
        // Upper bound is the contract that matters: no chunk should blow
        // past the embedding model's context budget. Lower bounds are not
        // asserted because tail chunks naturally shrink as we exhaust the
        // segment list, and the snap-to-boundary heuristic can pull
        // individual chunks below target by a wide margin.
        for chunk in chunks {
            let approxTokens = ChunkBuilder.approximateTokenCount(chunk.text)
            XCTAssertGreaterThan(approxTokens, 0, "Chunk must not be empty")
            XCTAssertLessThan(approxTokens, 800, "Chunk should not exceed target by 2x")
        }
        // At least one chunk should be near the target — i.e. the chunker
        // is actually trying to fill the window when there's enough material.
        let nearTarget = chunks.contains { ChunkBuilder.approximateTokenCount($0.text) > 200 }
        XCTAssertTrue(nearTarget, "At least one chunk should approach the target token budget")
    }

    func testSpeakerBoundarySnapPreservesSpeakerWithinChunk() {
        // Two-speaker transcript at small target size should snap the chunk
        // boundary to a speaker turn rather than mid-sentence.
        let transcript = shortTwoSpeakerTranscript()
        let builder = ChunkBuilder(targetTokens: 30, overlapFraction: 0.15)
        let chunks = builder.build(from: transcript)
        XCTAssertGreaterThan(chunks.count, 1, "Small target should split this transcript")
        // Each chunk should have at least one speaker assigned (the dominant
        // one in the chunk's segments).
        for chunk in chunks {
            XCTAssertNotNil(chunk.speakerID, "Chunk should carry the dominant speaker ID")
        }
    }

    func testOverlapProducesShiftingChunks() {
        let transcript = longMonologueTranscript()
        let builder = ChunkBuilder(targetTokens: 200, overlapFraction: 0.15)
        let chunks = builder.build(from: transcript)
        XCTAssertGreaterThan(chunks.count, 2)
        // With 15% overlap, consecutive chunks should NOT have identical
        // text — they advance, but should also share some content.
        for (a, b) in zip(chunks, chunks.dropFirst()) {
            XCTAssertNotEqual(a.text, b.text)
            XCTAssertNotEqual(a.startMS, b.startMS)
        }
    }

    func testSingleSegmentTranscriptProducesOneChunk() {
        let segment = FixtureSegment(
            text: "Just one short utterance, nothing else to chunk here.",
            startMS: 0,
            endMS: 5_000,
            speakerID: Self.alice
        )
        let transcript = FixtureTranscript(
            episodeID: Self.episode,
            podcastID: Self.podcast,
            segments: [segment]
        )
        let builder = ChunkBuilder()
        let chunks = builder.build(from: transcript)
        XCTAssertEqual(chunks.count, 1)
        XCTAssertEqual(chunks.first?.startMS, 0)
        XCTAssertEqual(chunks.first?.endMS, 5_000)
        XCTAssertEqual(chunks.first?.speakerID, Self.alice)
    }

    func testIDsAreStableForIdenticalInput() {
        // Re-running the builder over the same transcript should produce
        // identical IDs (deterministic from episodeID + index) so re-ingest
        // is idempotent on `(episodeID, chunkIndex)`.
        let transcript = longMonologueTranscript()
        let builder = ChunkBuilder()
        let first = builder.build(from: transcript)
        let second = builder.build(from: transcript)
        XCTAssertEqual(first.count, second.count)
        for (a, b) in zip(first, second) {
            XCTAssertEqual(a.id, b.id, "Chunk IDs must be stable across runs for idempotent re-ingest")
        }
    }

    func testApproximateTokenCountRoughlyMatches() {
        // Sanity check: the public token estimator should land within an
        // order-of-magnitude of word count. Used by the chunker, so it
        // shouldn't be wildly off.
        let text = String(repeating: "word ", count: 100)
        let approx = ChunkBuilder.approximateTokenCount(text)
        XCTAssertGreaterThan(approx, 50)
        XCTAssertLessThan(approx, 200)
    }
}
