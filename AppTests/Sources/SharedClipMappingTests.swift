import Pod0Core
import XCTest
@testable import Podcastr

final class SharedClipMappingTests: XCTestCase {
    func testUnsupportedFutureClipSourceKeepsTheSavedClipVisible() {
        XCTAssertEqual(record(source: .unsupported(wireCode: 41)).swiftValue?.source, .unsupported)
    }

    func testInvalidClipBoundsFailClosedInNativeProjection() {
        XCTAssertNil(record(startMilliseconds: 20, endMilliseconds: 20).swiftValue)
        XCTAssertNil(record(startMilliseconds: 21, endMilliseconds: 20).swiftValue)
    }

    func testInvalidClipRevisionFailsClosedInNativeProjection() {
        XCTAssertNil(record(revision: 0).swiftValue)
    }

    func testNativeClipRejectsStringlySpeakerLabelsAtTheTypedBoundary() {
        let clip = Clip(
            episodeID: UUID(),
            subscriptionID: UUID(),
            startMs: 10,
            endMs: 20,
            speakerID: "Speaker One"
        )
        XCTAssertThrowsError(try clip.coreSpeakerID()) { error in
            XCTAssertEqual(error as? SharedClipMappingError, .invalidSpeaker)
        }
        XCTAssertNil(try clip.coreSpeakerID(preservingLegacyLabel: true))
    }

    func testLegacySpeakerLabelRemainsVisibleInTheNativeProjection() {
        XCTAssertEqual(record(speakerLabel: "Speaker One").swiftValue?.speakerID, "Speaker One")
    }

    private func record(
        revision: UInt64 = 1,
        startMilliseconds: UInt64 = 10,
        endMilliseconds: UInt64 = 20,
        source: Pod0Core.ClipSource = .touch,
        speakerLabel: String? = nil
    ) -> ClipRecord {
        ClipRecord(
            clipId: ClipId(high: 1, low: 2),
            revision: ClipRevision(value: revision),
            episodeId: EpisodeId(high: 3, low: 4),
            podcastId: PodcastId(high: 5, low: 6),
            startMilliseconds: startMilliseconds,
            endMilliseconds: endMilliseconds,
            createdAt: UnixTimestampMilliseconds(value: 1_700_000_000_000),
            caption: nil,
            speakerId: nil,
            speakerLabel: speakerLabel,
            frozenTranscriptText: "Exact words",
            source: source,
            deleted: false,
            evidence: nil
        )
    }
}
