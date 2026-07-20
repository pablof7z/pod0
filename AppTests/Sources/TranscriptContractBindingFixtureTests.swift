import Pod0Core
import XCTest

final class TranscriptContractBindingFixtureTests: XCTestCase {
    func testSwiftQualifiesCanonicalTranscriptGoldenFixture() throws {
        let fixture = try loadFixture()
        let request = try makeRequest(fixture)
        let qualifiedSegments = projectTranscriptContract(
            request: request,
            scope: .segments,
            offset: 0,
            maxItems: 1
        )
        guard case let .qualified(receipt: receipt, transcript: segments) = qualifiedSegments else {
            return XCTFail("Valid transcript fixture was rejected")
        }
        let artifactID = try id("expected_artifact_id", fixture, TranscriptArtifactId.init)
        let versionID = try id(
            "expected_transcript_version_id",
            fixture,
            TranscriptVersionId.init
        )

        XCTAssertEqual(fixture["fixture_version"], "1")
        XCTAssertEqual(UInt32(fixture["contract_version"] ?? ""), 18)
        XCTAssertEqual(fixture["unknown_future_field"], "ignored-by-v1-readers")
        XCTAssertEqual(receipt.artifactId, artifactID)
        XCTAssertEqual(receipt.transcriptVersionId, versionID)
        XCTAssertEqual(receipt.transcriptContentDigest, try digest("expected_content_digest", fixture))
        XCTAssertEqual(receipt.artifactIntegrityDigest, try digest("expected_integrity_digest", fixture))
        XCTAssertEqual(receipt.commandFingerprint, try digest("expected_command_fingerprint", fixture))
        XCTAssertEqual(
            receipt.selectionRevision.value,
            try number("expected_committed_selection_revision", fixture)
        )
        XCTAssertEqual(receipt.speakerCount, UInt32(fixture["speaker_count"] ?? ""))
        XCTAssertEqual(receipt.segmentCount, UInt32(fixture["segment_count"] ?? ""))
        XCTAssertEqual(receipt.wordCount, try number("expected_word_count", fixture))

        let expectedSegmentID = try id(
            "expected_segment_0_id",
            fixture,
            TranscriptSegmentId.init
        )
        XCTAssertEqual(segments.segments.map(\.segmentId), [expectedSegmentID])
        XCTAssertEqual(segments.segments.first?.text, fixture["segment_0_text"])
        XCTAssertTrue(segments.hasMore)
        guard case .unsupported(let wireCode) = try XCTUnwrap(segments.summary).source else {
            return XCTFail("Future transcript source was not preserved")
        }
        XCTAssertEqual(wireCode, UInt32(fixture["source_wire_code"] ?? ""))

        let secondSegmentID = try id(
            "expected_segment_1_id",
            fixture,
            TranscriptSegmentId.init
        )
        let qualifiedWords = projectTranscriptContract(
            request: request,
            scope: .words(segmentId: secondSegmentID),
            offset: 0,
            maxItems: 20
        )
        guard case let .qualified(receipt: _, transcript: words) = qualifiedWords else {
            return XCTFail("Valid transcript word fixture was rejected")
        }
        XCTAssertEqual(
            words.words.last?.endMilliseconds,
            try number("segment_1_word_2_end_milliseconds", fixture)
        )
    }

    private func makeRequest(_ fixture: [String: String]) throws -> TranscriptCommitRequest {
        let speakerCount = Int(try number("speaker_count", fixture))
        let speakers = try (0..<speakerCount).map { index in
            let prefix = "speaker_\(index)"
            return TranscriptArtifactSpeakerInput(
                speakerId: try id("\(prefix)_id", fixture, SpeakerId.init),
                label: try value("\(prefix)_label", fixture),
                displayName: fixture["\(prefix)_display_name"]
            )
        }
        let segmentCount = Int(try number("segment_count", fixture))
        let segments = try (0..<segmentCount).map { index in
            let prefix = "segment_\(index)"
            let speakerIndex = Int(try number("\(prefix)_speaker_index", fixture))
            let wordCount = Int(try number("\(prefix)_word_count", fixture))
            let words = try (0..<wordCount).map { wordIndex in
                let word = "\(prefix)_word_\(wordIndex)"
                return TranscriptArtifactWordInput(
                    text: try value("\(word)_text", fixture),
                    startMilliseconds: try number("\(word)_start_milliseconds", fixture),
                    endMilliseconds: try number("\(word)_end_milliseconds", fixture)
                )
            }
            return TranscriptArtifactSegmentInput(
                text: try value("\(prefix)_text", fixture),
                startMilliseconds: try number("\(prefix)_start_milliseconds", fixture),
                endMilliseconds: try number("\(prefix)_end_milliseconds", fixture),
                speakerId: speakers[speakerIndex].speakerId,
                words: words
            )
        }
        return TranscriptCommitRequest(
            commandId: try id("command_id", fixture, CommandId.init),
            expectedSelectionRevision: StateRevision(
                value: try number("expected_selection_revision", fixture)
            ),
            artifact: TranscriptArtifactInput(
                episodeId: try id("episode_id", fixture, EpisodeId.init),
                podcastId: try id("podcast_id", fixture, PodcastId.init),
                sourceRevision: try value("source_revision", fixture),
                source: .unsupported(wireCode: UInt32(try number("source_wire_code", fixture))),
                provider: fixture["provider"],
                sourcePayloadDigest: try digest("source_payload_digest", fixture),
                language: try value("language", fixture),
                generatedAt: UnixTimestampMilliseconds(
                    value: Int64(try number("generated_at_milliseconds", fixture))
                ),
                speakers: speakers,
                segments: segments
            )
        )
    }

    private func loadFixture() throws -> [String: String] {
        let url = try XCTUnwrap(Bundle(for: Self.self).url(
            forResource: "transcript-contract-v1",
            withExtension: "properties"
        ))
        return try String(contentsOf: url, encoding: .utf8)
            .split(whereSeparator: \.isNewline)
            .filter { !$0.isEmpty && !$0.hasPrefix("#") }
            .reduce(into: [:]) { values, line in
                let parts = line.split(separator: "=", maxSplits: 1, omittingEmptySubsequences: false)
                guard parts.count == 2 else { return }
                values[String(parts[0])] = String(parts[1])
            }
    }

    private func value(_ key: String, _ fixture: [String: String]) throws -> String {
        try XCTUnwrap(fixture[key], "Missing fixture value: \(key)")
    }

    private func number(_ key: String, _ fixture: [String: String]) throws -> UInt64 {
        try XCTUnwrap(UInt64(fixture[key] ?? ""), "Missing numeric fixture value: \(key)")
    }

    private func id<T>(
        _ prefix: String,
        _ fixture: [String: String],
        _ build: (UInt64, UInt64) -> T
    ) throws -> T {
        build(
            try number("\(prefix)_high", fixture),
            try number("\(prefix)_low", fixture)
        )
    }

    private func digest(_ prefix: String, _ fixture: [String: String]) throws -> ContentDigest {
        ContentDigest(
            word0: try number("\(prefix)_word_0", fixture),
            word1: try number("\(prefix)_word_1", fixture),
            word2: try number("\(prefix)_word_2", fixture),
            word3: try number("\(prefix)_word_3", fixture)
        )
    }
}
