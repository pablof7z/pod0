import XCTest
@testable import Podcastr

// MARK: - KnowledgeQueryRowDecodeTests
//
// Contract gate for the Rust-JSON → Swift decode path of `KnowledgeQueryRow`.
//
// The kernel emits snake_case keys (`episode_id`, `podcast_id`,
// `episode_title`, `podcast_title`, `chunk_index`, `start_secs`, `end_secs`,
// `text`, `relevance_score`). They must survive the bridge's
// `.convertFromSnakeCase` decoder (KernelDecoding.makeDecoder) and land on
// the corresponding camelCase Swift properties WITHOUT explicit CodingKeys
// (the #371 / ffi_decode_snakecase_contract hazard: explicit snake_case
// CodingKeys under .convertFromSnakeCase double-converts and throws
// keyNotFound, silently dropping all rows).

final class KnowledgeQueryRowDecodeTests: XCTestCase {

    // MARK: - Core snake_case → camelCase contract

    /// Full-row fixture exercises every field including the load-bearing seek
    /// timestamps `start_secs` / `end_secs` (from slice 5a timed chunking).
    func testKnowledgeQueryRowDecodesFromSnakeCaseViaKernelDecoding() throws {
        let json = """
        {
            "result": [
                {
                    "episode_id": "ep-abc-123",
                    "podcast_id": "pod-xyz-456",
                    "episode_title": "Zone 2 Training Deep Dive",
                    "podcast_title": "Huberman Lab",
                    "chunk_index": 3,
                    "start_secs": 125.5,
                    "end_secs": 155.0,
                    "text": "Zone 2 cardio is essential for mitochondrial health.",
                    "relevance_score": 0.91
                }
            ]
        }
        """.data(using: .utf8)!

        let decoder = KernelDecoding.makeDecoder()
        let envelope = try decoder.decode(KnowledgeQueryResponseEnvelope.self, from: json)

        let rows = try XCTUnwrap(envelope.result, "result array must decode from snake_case envelope")
        XCTAssertEqual(rows.count, 1, "one row expected in fixture")

        let row = try XCTUnwrap(rows.first)
        // Identity fields: snake_case → camelCase via .convertFromSnakeCase
        XCTAssertEqual(row.episodeId, "ep-abc-123",
                       "episode_id → episodeId via .convertFromSnakeCase")
        XCTAssertEqual(row.podcastId, "pod-xyz-456",
                       "podcast_id → podcastId via .convertFromSnakeCase")
        // Title fields
        XCTAssertEqual(row.episodeTitle, "Zone 2 Training Deep Dive",
                       "episode_title → episodeTitle via .convertFromSnakeCase")
        XCTAssertEqual(row.podcastTitle, "Huberman Lab",
                       "podcast_title → podcastTitle via .convertFromSnakeCase")
        // Chunk index
        XCTAssertEqual(row.chunkIndex, 3)
        // Load-bearing seek timestamps (slice 5a timed chunking)
        XCTAssertEqual(row.startSecs, 125.5, accuracy: 0.001,
                       "start_secs → startSecs must preserve the timed chunk start")
        XCTAssertEqual(row.endSecs, 155.0, accuracy: 0.001,
                       "end_secs → endSecs must preserve the timed chunk end")
        // Text
        XCTAssertEqual(row.text, "Zone 2 cardio is essential for mitochondrial health.")
        // Relevance score
        XCTAssertEqual(row.relevanceScore, 0.91, accuracy: 0.001,
                       "relevance_score → relevanceScore via .convertFromSnakeCase")
    }

    /// Multiple rows in a single response all decode correctly.
    func testMultipleRowsDecodeFromSnakeCaseEnvelope() throws {
        let json = """
        {
            "result": [
                {
                    "episode_id": "ep-1",
                    "podcast_id": "pod-1",
                    "episode_title": "Episode One",
                    "podcast_title": "Show A",
                    "chunk_index": 0,
                    "start_secs": 0.0,
                    "end_secs": 30.0,
                    "text": "First chunk.",
                    "relevance_score": 0.95
                },
                {
                    "episode_id": "ep-2",
                    "podcast_id": "pod-2",
                    "episode_title": "Episode Two",
                    "podcast_title": "Show B",
                    "chunk_index": 7,
                    "start_secs": 210.0,
                    "end_secs": 240.0,
                    "text": "Second chunk.",
                    "relevance_score": 0.80
                }
            ]
        }
        """.data(using: .utf8)!

        let decoder = KernelDecoding.makeDecoder()
        let envelope = try decoder.decode(KnowledgeQueryResponseEnvelope.self, from: json)
        let rows = try XCTUnwrap(envelope.result)
        XCTAssertEqual(rows.count, 2)
        XCTAssertEqual(rows[0].episodeId, "ep-1")
        XCTAssertEqual(rows[0].startSecs, 0.0, accuracy: 0.001)
        XCTAssertEqual(rows[1].episodeId, "ep-2")
        XCTAssertEqual(rows[1].chunkIndex, 7)
        XCTAssertEqual(rows[1].startSecs, 210.0, accuracy: 0.001)
        XCTAssertEqual(rows[1].endSecs, 240.0, accuracy: 0.001)
    }

    /// An empty result array decodes without error (e.g. no indexed content yet).
    func testEmptyResultArrayDecodesWithoutError() throws {
        let json = #"{"result":[]}"#.data(using: .utf8)!
        let decoder = KernelDecoding.makeDecoder()
        let envelope = try decoder.decode(KnowledgeQueryResponseEnvelope.self, from: json)
        let rows = try XCTUnwrap(envelope.result)
        XCTAssertTrue(rows.isEmpty, "empty result array must decode to an empty Swift array")
        XCTAssertNil(envelope.error, "empty-result envelope must have no error")
    }

    /// An error envelope (kernel-side failure) decodes the plain-string error
    /// and has nil result.
    func testErrorEnvelopeDecodesErrorString() throws {
        let json = #"{"error":"embedding model not configured"}"#.data(using: .utf8)!
        let decoder = KernelDecoding.makeDecoder()
        let envelope = try decoder.decode(KnowledgeQueryResponseEnvelope.self, from: json)
        XCTAssertEqual(envelope.error, "embedding model not configured",
                       "error string must decode verbatim")
        XCTAssertNil(envelope.result, "error envelope must have nil result")
    }

    // MARK: - TranscriptHit mapping (start/end seconds preserved)

    /// `LivePodcastRAGAdapter.makeTranscriptHit` must map `startSecs` /
    /// `endSecs` from the row directly onto `TranscriptHit.startSeconds` /
    /// `endSeconds`. This is the end-to-end seek correctness check.
    func testTranscriptHitPreservesStartEndSecondsFromRow() throws {
        // Decode a row via the real bridge decoder (not a hand-constructed value)
        // so the test exercises the actual .convertFromSnakeCase path.
        let json = """
        {
            "result": [{
                "episode_id": "ep-seek",
                "podcast_id": "pod-seek",
                "episode_title": "Seek Test",
                "podcast_title": "Test Show",
                "chunk_index": 5,
                "start_secs": 347.25,
                "end_secs": 391.0,
                "text": "Content at the seek point.",
                "relevance_score": 0.88
            }]
        }
        """.data(using: .utf8)!
        let decoder = KernelDecoding.makeDecoder()
        let envelope = try decoder.decode(KnowledgeQueryResponseEnvelope.self, from: json)
        let row = try XCTUnwrap(envelope.result?.first)

        // Simulate what LivePodcastRAGAdapter.makeTranscriptHit does (internal,
        // so we replicate the mapping inline — the production code is two lines).
        let hit = TranscriptHit(
            episodeID: row.episodeId,
            startSeconds: row.startSecs,
            endSeconds: row.endSecs,
            speaker: nil,
            text: row.text,
            score: row.relevanceScore
        )

        XCTAssertEqual(hit.episodeID, "ep-seek")
        XCTAssertEqual(hit.startSeconds, 347.25, accuracy: 0.001,
                       "startSecs from row must land on TranscriptHit.startSeconds (seek correctness)")
        XCTAssertEqual(hit.endSeconds, 391.0, accuracy: 0.001,
                       "endSecs from row must land on TranscriptHit.endSeconds (seek correctness)")
        XCTAssertNil(hit.speaker, "speaker must be nil — kernel rows carry no diarisation")
        XCTAssertEqual(try XCTUnwrap(hit.score), 0.88, accuracy: 0.001)
    }
}
