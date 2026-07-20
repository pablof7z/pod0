import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class SharedPlaybackMappingTests: XCTestCase {
    func testQueueIdentitySegmentAndLabelRoundTripWithoutLoss() throws {
        let item = QueueItem(
            id: UUID(uuidString: "31000000-0000-0000-0000-000000000001")!,
            episodeID: UUID(uuidString: "32000000-0000-0000-0000-000000000001")!,
            startSeconds: 12.345,
            endSeconds: 67.89,
            label: "Evidence"
        )

        let restored = try XCTUnwrap(item.coreValue.swiftValue)

        XCTAssertEqual(restored.id, item.id)
        XCTAssertEqual(restored.episodeID, item.episodeID)
        XCTAssertEqual(try XCTUnwrap(restored.startSeconds), 12.345, accuracy: 0.001)
        XCTAssertEqual(try XCTUnwrap(restored.endSeconds), 67.89, accuracy: 0.001)
        XCTAssertEqual(restored.label, item.label)
    }

    func testSleepModesUsePlatformNeutralMillisecondContract() {
        XCTAssertEqual(
            PlaybackSleepTimer.minutes(15).coreValue,
            .duration(durationMilliseconds: 900_000)
        )
        XCTAssertEqual(
            PlaybackSleepMode.duration(durationMilliseconds: 60_001).swiftValue,
            .minutes(2)
        )
        XCTAssertEqual(PlaybackSleepTimer.endOfEpisode.coreValue, .endOfEpisode)
        XCTAssertEqual(PlaybackSleepMode.endOfEpisode.swiftValue, .endOfEpisode)
        XCTAssertEqual(PlaybackSleepMode.unsupported(wireCode: 99).swiftValue, .off)
    }

    func testPlaybackProjectionAppliesStableQueueAndActiveSegmentToNativeState() {
        let episode = Episode(
            id: UUID(uuidString: "33000000-0000-0000-0000-000000000001")!,
            podcastID: UUID(),
            guid: "shared-playback",
            title: "Shared Playback",
            pubDate: Date(),
            enclosureURL: URL(string: "https://example.test/shared.mp3")!
        )
        let queued = QueueItem(
            id: UUID(uuidString: "34000000-0000-0000-0000-000000000001")!,
            episodeID: episode.id,
            startSeconds: 90,
            endSeconds: 120,
            label: "Queue segment"
        )
        let state = PlaybackState(engine: AudioEngine())
        let projection = PlaybackProjection(
            current: PlaybackItem(
                episodeId: EpisodeId(uuid: episode.id),
                title: episode.title,
                durableResumePositionMilliseconds: 45_000,
                meaningfulListeningReached: false,
                segment: PlaybackSegment(
                    startPositionMilliseconds: 10_000,
                    endPositionMilliseconds: 20_000
                ),
                label: "Current segment",
                completed: false,
                policyState: .paused,
                chapterContext: nil
            ),
            queue: [queued.coreValue],
            rate: PlaybackRatePermille(value: 1_250),
            sleepMode: .endOfEpisode,
            autoMarkPlayedAtNaturalEnd: true,
            autoPlayNext: true,
            autoSkipAds: false,
            allowedActions: PlaybackAllowedActions(
                canPlay: true,
                canPause: false,
                canSeek: true,
                canAdvance: true
            ),
            hostState: .prepared,
            operations: []
        )

        state.applySharedPlayback(projection, stateRevision: 7) {
            id in id == episode.id ? episode : nil
        }

        XCTAssertEqual(state.episode?.id, episode.id)
        XCTAssertEqual(state.currentSegmentEndTime, 20)
        XCTAssertEqual(state.queue, [queued])
        XCTAssertEqual(state.sleepTimer, .endOfEpisode)
        XCTAssertNil(state.engine.episode)
    }
}
