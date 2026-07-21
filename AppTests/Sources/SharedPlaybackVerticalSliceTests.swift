import Foundation
import Observation
import XCTest
@testable import Podcastr

@MainActor
final class SharedPlaybackVerticalSliceTests: XCTestCase {
    func testSharedPlaybackSurvivesRelaunchWithRustAsSoleWriter() async throws {
        let fileURL = AppStateTestSupport.uniqueTempFileURL()
        let persistence = Persistence(fileURL: fileURL)
        defer { persistence.reset() }
        let podcast = Podcast(
            id: UUID(),
            feedURL: URL(string: "https://playback.example/feed.xml")!,
            title: "Playback Show",
            discoveredAt: Date(timeIntervalSince1970: 1_700_000_000)
        )
        let first = episode(podcastID: podcast.id, number: 1, position: 33)
        let second = episode(podcastID: podcast.id, number: 2, position: 0)
        var legacy = AppState()
        legacy.podcasts = [podcast]
        legacy.subscriptions = [PodcastSubscription(podcastID: podcast.id)]
        legacy.episodes = [first, second]
        legacy.lastPlayedEpisodeID = first.id
        XCTAssertTrue(persistence.write(legacy, revision: 1))

        let store = makeStore(persistence)
        let playback = PlaybackState(engine: AudioEngine())
        let client = try XCTUnwrap(store.sharedLibrary)
        client.attachPlayback(playback, store: store)
        playback.enqueue(second.id)
        let remoteSeek = playback.engine.nowPlaying.performRemoteCommand(.seek(47))
        XCTAssertEqual(remoteSeek, .success)
        playback.setRate(.fast)
        playback.setSleepTimer(.minutes(15))
        await waitFor("playback controls to reach the native host") {
            playback.queue.first?.episodeID == second.id
                && abs(playback.engine.currentTime - 47) <= 0.001
                && abs(playback.engine.rate - 1.5) <= 0.001
                && playback.engine.sleepTimer.mode == .duration(900)
                && store.episode(id: first.id)?.playbackPosition == 47
        }

        let queuedID = try XCTUnwrap(playback.queue.first?.id)
        XCTAssertEqual(playback.queue.first?.episodeID, second.id)
        XCTAssertEqual(playback.engine.currentTime, 47, accuracy: 0.001)
        XCTAssertEqual(playback.engine.rate, 1.5, accuracy: 0.001)
        XCTAssertEqual(playback.engine.sleepTimer.mode, .duration(900))
        XCTAssertEqual(store.episode(id: first.id)?.playbackPosition, 47)

        // A real duration expiry clears the native timer before delivering
        // the callback. Mirror that ordering without waiting on wall time.
        playback.engine.sleepTimer.cancel()
        playback.engine.sleepTimer.onFire()
        await waitFor("the fired timer to clear in Rust and native state") {
            playback.sleepTimer == .off && playback.engine.sleepTimer.mode == .off
        }
        XCTAssertEqual(playback.sleepTimer, .off)
        XCTAssertEqual(playback.engine.sleepTimer.mode, .off)

        client.shutdown()
        let relaunched = makeStore(persistence)
        let restoredPlayback = PlaybackState(engine: AudioEngine())
        try XCTUnwrap(relaunched.sharedLibrary).attachPlayback(
            restoredPlayback,
            store: relaunched
        )
        await waitFor("durable playback to restore after relaunch") {
            relaunched.episode(id: first.id)?.playbackPosition == 47
                && restoredPlayback.episode?.id == first.id
                && abs(restoredPlayback.engine.currentTime - 47) <= 0.001
                && abs(restoredPlayback.engine.rate - 1.5) <= 0.001
                && restoredPlayback.sleepTimer == .off
                && restoredPlayback.engine.sleepTimer.mode == .off
                && restoredPlayback.queue.first?.episodeID == second.id
        }

        XCTAssertEqual(relaunched.episode(id: first.id)?.playbackPosition, 47)
        XCTAssertEqual(restoredPlayback.episode?.id, first.id)
        XCTAssertEqual(restoredPlayback.engine.currentTime, 47, accuracy: 0.001)
        XCTAssertEqual(restoredPlayback.engine.rate, 1.5, accuracy: 0.001)
        XCTAssertEqual(restoredPlayback.sleepTimer, .off)
        XCTAssertEqual(restoredPlayback.engine.sleepTimer.mode, .off)
        XCTAssertEqual(restoredPlayback.queue.first?.id, queuedID)
        XCTAssertEqual(restoredPlayback.queue.first?.episodeID, second.id)

        relaunched.markEpisodePlayed(first.id)
        await waitFor("played state to commit") {
            relaunched.episode(id: first.id)?.played == true
                && relaunched.episode(id: first.id)?.playbackPosition == 0
        }
        XCTAssertEqual(relaunched.episode(id: first.id)?.played, true)
        XCTAssertEqual(relaunched.episode(id: first.id)?.playbackPosition, 0)
        relaunched.markEpisodeUnplayed(first.id)
        await waitFor("unplayed state to commit") {
            relaunched.episode(id: first.id)?.played == false
        }
        XCTAssertEqual(relaunched.episode(id: first.id)?.played, false)
    }

    func testSharedQueueCommandsRoundTripThroughCoreProjections() async throws {
        let fileURL = AppStateTestSupport.uniqueTempFileURL()
        let persistence = Persistence(fileURL: fileURL)
        defer { persistence.reset() }
        let podcast = Podcast(
            id: UUID(),
            feedURL: URL(string: "https://queue.example/feed.xml")!,
            title: "Queue Show",
            discoveredAt: Date(timeIntervalSince1970: 1_700_000_000)
        )
        let first = episode(podcastID: podcast.id, number: 1, position: 12)
        let second = episode(podcastID: podcast.id, number: 2, position: 0)
        let third = episode(podcastID: podcast.id, number: 3, position: 0)
        var legacy = AppState()
        legacy.podcasts = [podcast]
        legacy.subscriptions = [PodcastSubscription(podcastID: podcast.id)]
        legacy.episodes = [first, second, third]
        legacy.lastPlayedEpisodeID = first.id
        XCTAssertTrue(persistence.write(legacy, revision: 1))

        let store = makeStore(persistence)
        let playback = PlaybackState(engine: AudioEngine())
        let client = try XCTUnwrap(store.sharedLibrary)
        client.attachPlayback(playback, store: store)

        playback.enqueue(second.id)
        playback.enqueue(third.id)
        await waitFor("queue appends to commit") {
            playback.queue.map(\.episodeID) == [second.id, third.id]
        }
        XCTAssertEqual(playback.queue.map(\.episodeID), [second.id, third.id])

        playback.moveQueue(from: IndexSet(integer: 0), to: 2)
        await waitFor("queue reorder to commit") {
            playback.queue.map(\.episodeID) == [third.id, second.id]
        }
        XCTAssertEqual(playback.queue.map(\.episodeID), [third.id, second.id])

        let thirdSlot = try XCTUnwrap(playback.queue.first?.id)
        playback.removeFromQueue(itemID: thirdSlot)
        await waitFor("queue removal to commit") {
            playback.queue.map(\.episodeID) == [second.id]
        }
        XCTAssertEqual(playback.queue.map(\.episodeID), [second.id])

        playback.clearQueue()
        await waitFor("queue clear to commit") { playback.queue.isEmpty }
        XCTAssertTrue(playback.queue.isEmpty)

        client.shutdown()
        let relaunched = makeStore(persistence)
        let restored = PlaybackState(engine: AudioEngine())
        try XCTUnwrap(relaunched.sharedLibrary).attachPlayback(restored, store: relaunched)
        await waitFor("cleared queue to restore after relaunch") {
            restored.episode?.id == first.id && restored.queue.isEmpty
        }
        XCTAssertTrue(restored.queue.isEmpty)
    }

    private func makeStore(_ persistence: Persistence) -> AppStateStore {
        AppStateStore(
            persistence: persistence,
            sharedFeedHost: QueuedCoreFeedHost([]),
            startSubscriptionRefresh: false
        )
    }

    private func episode(
        podcastID: UUID,
        number: Int,
        position: TimeInterval
    ) -> Episode {
        Episode(
            id: UUID(),
            podcastID: podcastID,
            guid: "playback-\(number)",
            title: "Episode \(number)",
            pubDate: Date(timeIntervalSince1970: 1_700_000_000 + Double(number)),
            duration: 600,
            enclosureURL: URL(string: "https://playback.example/\(number).mp3")!,
            playbackPosition: position
        )
    }

    private func waitFor(
        _ description: String,
        timeout: TimeInterval = 5,
        condition: @escaping @MainActor () -> Bool
    ) async {
        let completed = expectation(description: description)
        let waiter = ObservableConditionWaiter(condition: condition) {
            completed.fulfill()
        }
        waiter.start()
        await fulfillment(of: [completed], timeout: timeout)
        withExtendedLifetime(waiter) {}
    }
}

@MainActor
private final class ObservableConditionWaiter {
    private let condition: @MainActor () -> Bool
    private let completion: @MainActor () -> Void
    private var completed = false

    init(
        condition: @escaping @MainActor () -> Bool,
        completion: @escaping @MainActor () -> Void
    ) {
        self.condition = condition
        self.completion = completion
    }

    func start() {
        guard !completed else { return }
        let satisfied = withObservationTracking {
            condition()
        } onChange: { [weak self] in
            Task { @MainActor [weak self] in self?.start() }
        }
        guard satisfied, !completed else { return }
        completed = true
        completion()
    }
}
