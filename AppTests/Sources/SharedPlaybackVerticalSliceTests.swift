import Foundation
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
        await drainProjectionDeliveries()

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
        await drainProjectionDeliveries()
        XCTAssertEqual(playback.sleepTimer, .off)
        XCTAssertEqual(playback.engine.sleepTimer.mode, .off)

        let relaunched = makeStore(persistence)
        let restoredPlayback = PlaybackState(engine: AudioEngine())
        try XCTUnwrap(relaunched.sharedLibrary).attachPlayback(
            restoredPlayback,
            store: relaunched
        )
        await drainProjectionDeliveries()

        XCTAssertEqual(relaunched.episode(id: first.id)?.playbackPosition, 47)
        XCTAssertEqual(restoredPlayback.episode?.id, first.id)
        XCTAssertEqual(restoredPlayback.engine.currentTime, 47, accuracy: 0.001)
        XCTAssertEqual(restoredPlayback.engine.rate, 1.5, accuracy: 0.001)
        XCTAssertEqual(restoredPlayback.sleepTimer, .off)
        XCTAssertEqual(restoredPlayback.engine.sleepTimer.mode, .off)
        XCTAssertEqual(restoredPlayback.queue.first?.id, queuedID)
        XCTAssertEqual(restoredPlayback.queue.first?.episodeID, second.id)

        relaunched.markEpisodePlayed(first.id)
        await drainProjectionDeliveries()
        XCTAssertEqual(relaunched.episode(id: first.id)?.played, true)
        XCTAssertEqual(relaunched.episode(id: first.id)?.playbackPosition, 0)
        relaunched.markEpisodeUnplayed(first.id)
        await drainProjectionDeliveries()
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
        try XCTUnwrap(store.sharedLibrary).attachPlayback(playback, store: store)

        playback.enqueue(second.id)
        playback.enqueue(third.id)
        await drainProjectionDeliveries()
        XCTAssertEqual(playback.queue.map(\.episodeID), [second.id, third.id])

        playback.moveQueue(from: IndexSet(integer: 0), to: 2)
        await drainProjectionDeliveries()
        XCTAssertEqual(playback.queue.map(\.episodeID), [third.id, second.id])

        let thirdSlot = try XCTUnwrap(playback.queue.first?.id)
        playback.removeFromQueue(itemID: thirdSlot)
        await drainProjectionDeliveries()
        XCTAssertEqual(playback.queue.map(\.episodeID), [second.id])

        playback.clearQueue()
        await drainProjectionDeliveries()
        XCTAssertTrue(playback.queue.isEmpty)

        let relaunched = makeStore(persistence)
        let restored = PlaybackState(engine: AudioEngine())
        try XCTUnwrap(relaunched.sharedLibrary).attachPlayback(restored, store: relaunched)
        await drainProjectionDeliveries()
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

    private func drainProjectionDeliveries() async {
        await Task.yield()
        await Task.yield()
        await Task.yield()
    }
}
