import Pod0Core
import XCTest

final class SharedClipBindingFixtureTests: XCTestCase {
    func testSwiftDecodesClipProjectionGoldenFixture() throws {
        let fixtureURL = try XCTUnwrap(Bundle(for: Self.self).url(
            forResource: "clip-projection-v1",
            withExtension: "properties"
        ))
        let fixture = try decodeProperties(at: fixtureURL)
        func number(_ key: String) throws -> UInt64 {
            try XCTUnwrap(UInt64(fixture[key] ?? ""), "Missing numeric fixture value: \(key)")
        }
        let episodeID = EpisodeId(
            high: try number("episode_id_high"),
            low: try number("episode_id_low")
        )
        let evidence = ClipEvidenceReference(
            generationId: EvidenceGenerationId(
                high: try number("generation_id_high"),
                low: try number("generation_id_low")
            ),
            transcriptVersionId: TranscriptVersionId(
                high: try number("transcript_version_id_high"),
                low: try number("transcript_version_id_low")
            ),
            transcriptContentDigest: ContentDigest(
                word0: try number("content_digest_word_0"),
                word1: try number("content_digest_word_1"),
                word2: try number("content_digest_word_2"),
                word3: try number("content_digest_word_3")
            ),
            spanId: EvidenceSpanId(
                high: try number("span_id_high"),
                low: try number("span_id_low")
            )
        )
        let clip = ClipRecord(
            clipId: ClipId(high: try number("clip_id_high"), low: try number("clip_id_low")),
            revision: ClipRevision(value: try number("clip_revision")),
            episodeId: episodeID,
            podcastId: PodcastId(
                high: try number("podcast_id_high"),
                low: try number("podcast_id_low")
            ),
            startMilliseconds: try number("start_milliseconds"),
            endMilliseconds: try number("end_milliseconds"),
            createdAt: UnixTimestampMilliseconds(value: Int64(try number("created_at_milliseconds"))),
            caption: fixture["caption"],
            speakerId: SpeakerId(
                high: try number("speaker_id_high"),
                low: try number("speaker_id_low")
            ),
            speakerLabel: fixture["speaker_label"].flatMap { $0.isEmpty ? nil : $0 },
            frozenTranscriptText: try XCTUnwrap(fixture["frozen_transcript_text"]),
            source: .touch,
            deleted: false,
            evidence: evidence
        )
        let projection = ClipsProjection(
            scope: .clip(clipId: clip.clipId),
            collectionRevision: StateRevision(value: try number("collection_revision")),
            clips: [clip],
            operations: [],
            hasMore: false
        )

        XCTAssertEqual(UInt32(fixture["contract_version"] ?? ""), 17)
        XCTAssertEqual(fixture["source"], "touch")
        XCTAssertEqual(projection.clips.first?.frozenTranscriptText, fixture["frozen_transcript_text"])
        XCTAssertEqual(projection.clips.first?.evidence?.spanId, evidence.spanId)
    }

    private func decodeProperties(at url: URL) throws -> [String: String] {
        try String(contentsOf: url, encoding: .utf8)
            .split(whereSeparator: \.isNewline)
            .filter { !$0.isEmpty && !$0.hasPrefix("#") }
            .reduce(into: [:]) { result, line in
                let parts = line.split(separator: "=", maxSplits: 1, omittingEmptySubsequences: false)
                guard parts.count == 2 else { return }
                result[String(parts[0])] = String(parts[1])
            }
    }
}
