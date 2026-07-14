import Foundation
import XCTest
@testable import Podcastr

@MainActor
final class PersistenceDurabilityTests: XCTestCase {

    func testPersistenceSplitsLargeEpisodeStateAcrossSQLiteAndMetadata() async throws {
        let sharedFileURL = AppStateTestSupport.uniqueTempFileURL()
        defer { AppStateTestSupport.disposeIsolatedStore(at: sharedFileURL) }

        do {
            let made = AppStateTestSupport.makeIsolatedStore(fileURL: sharedFileURL)
            let firstStore = made.store
            let sub = makeSubscription(title: "Large State Show")
            firstStore.upsertPodcast(sub); XCTAssertTrue(firstStore.addSubscription(podcastID: sub.id))

            let padding = String(repeating: "x", count: 4_096)
            var episodes: [Episode] = []
            episodes.reserveCapacity(1_500)
            for i in 0..<1_500 {
                var ep = makeEpisode(podcastID: sub.id, guid: "large-\(i)")
                ep.description = padding
                episodes.append(ep)
            }
            firstStore.upsertEpisodes(episodes, forPodcast: sub.id)
            XCTAssertEqual(
                firstStore.persistence.lastEpisodeWriteSummary.kind,
                .replaceAll,
                "Large initial episode imports should keep using the full-rebuild path."
            )

            var settings = firstStore.state.settings
            settings.hasCompletedOnboarding = true
            firstStore.updateSettings(settings)

            let metadata = try Data(contentsOf: sharedFileURL)
            XCTAssertLessThan(
                metadata.count,
                512 * 1024,
                "Episode payloads should live in SQLite, not the JSON metadata file."
            )
            XCTAssertTrue(
                FileManager.default.fileExists(atPath: Persistence.episodeStoreURL(for: sharedFileURL).path)
            )
        }

        let reopened = AppStateTestSupport.makeIsolatedStore(fileURL: sharedFileURL, reset: false)
        XCTAssertTrue(reopened.store.state.settings.hasCompletedOnboarding)
        XCTAssertEqual(reopened.store.state.subscriptions.count, 1)
        XCTAssertEqual(reopened.store.state.episodes.count, 1_500)
    }

    func testSingleEpisodeMutationUsesDeltaWriteAndRoundTrips() async throws {
        let sharedFileURL = AppStateTestSupport.uniqueTempFileURL()
        defer { AppStateTestSupport.disposeIsolatedStore(at: sharedFileURL) }

        let targetID: UUID
        do {
            let made = AppStateTestSupport.makeIsolatedStore(fileURL: sharedFileURL)
            let store = made.store
            let sub = makeSubscription(title: "Delta Mutations")
            store.upsertPodcast(sub); XCTAssertTrue(store.addSubscription(podcastID: sub.id))
            let episodes = [
                makeEpisode(podcastID: sub.id, guid: "delta-1"),
                makeEpisode(podcastID: sub.id, guid: "delta-2"),
                makeEpisode(podcastID: sub.id, guid: "delta-3"),
            ]
            targetID = episodes[1].id
            store.upsertEpisodes(episodes, forPodcast: sub.id)

            store.persistence.resetEpisodeWriteSummary()
            store.setEpisodeStarred(targetID, true)

            let summary = store.persistence.lastEpisodeWriteSummary
            XCTAssertEqual(summary.kind, .delta)
            XCTAssertEqual(summary.upsertCount, 1)
            XCTAssertEqual(summary.deleteCount, 0)
            XCTAssertEqual(summary.sortOrderUpdateCount, 0)
        }

        let reopened = AppStateTestSupport.makeIsolatedStore(fileURL: sharedFileURL, reset: false)
        let target = try XCTUnwrap(reopened.store.state.episodes.first { $0.id == targetID })
        XCTAssertTrue(target.isStarred)
        XCTAssertEqual(reopened.store.state.episodes.count, 3)
    }

    func testSmallEpisodeDeleteUsesRowDeleteAndRoundTrips() async throws {
        let sharedFileURL = AppStateTestSupport.uniqueTempFileURL()
        defer { AppStateTestSupport.disposeIsolatedStore(at: sharedFileURL) }

        let deletedID: UUID
        let survivingIDs: [UUID]
        do {
            let made = AppStateTestSupport.makeIsolatedStore(fileURL: sharedFileURL)
            let store = made.store
            let sub = makeSubscription(title: "Delta Deletes")
            store.upsertPodcast(sub); XCTAssertTrue(store.addSubscription(podcastID: sub.id))
            let episodes = [
                makeEpisode(podcastID: sub.id, guid: "delete-1"),
                makeEpisode(podcastID: sub.id, guid: "delete-2"),
                makeEpisode(podcastID: sub.id, guid: "delete-3"),
            ]
            deletedID = episodes[1].id
            survivingIDs = [episodes[0].id, episodes[2].id]
            store.upsertEpisodes(episodes, forPodcast: sub.id)

            store.persistence.resetEpisodeWriteSummary()
            store.performMutationBatch {
                store.state.episodes.removeAll { $0.id == deletedID }
            }

            let summary = store.persistence.lastEpisodeWriteSummary
            XCTAssertEqual(summary.kind, .delta)
            XCTAssertEqual(summary.upsertCount, 0)
            XCTAssertEqual(summary.deleteCount, 1)
            XCTAssertEqual(summary.sortOrderUpdateCount, 1)
        }

        let reopened = AppStateTestSupport.makeIsolatedStore(fileURL: sharedFileURL, reset: false)
        XCTAssertNil(reopened.store.state.episodes.first { $0.id == deletedID })
        XCTAssertEqual(reopened.store.state.episodes.map(\.id), survivingIDs)
    }

    func testHasCompletedOnboardingPersistsAcrossStoreInstances() async throws {
        let sharedFileURL = AppStateTestSupport.uniqueTempFileURL()
        defer { AppStateTestSupport.disposeIsolatedStore(at: sharedFileURL) }

        do {
            let made = AppStateTestSupport.makeIsolatedStore(fileURL: sharedFileURL)
            var settings = made.store.state.settings
            settings.hasCompletedOnboarding = true
            made.store.updateSettings(settings)
        }

        let reopened = AppStateTestSupport.makeIsolatedStore(fileURL: sharedFileURL, reset: false)
        XCTAssertTrue(reopened.store.state.settings.hasCompletedOnboarding)
    }

    func testCorruptEpisodeSidecarDoesNotDiscardMetadata() async throws {
        let sharedFileURL = AppStateTestSupport.uniqueTempFileURL()
        defer { AppStateTestSupport.disposeIsolatedStore(at: sharedFileURL) }

        do {
            let made = AppStateTestSupport.makeIsolatedStore(fileURL: sharedFileURL)
            let sub = makeSubscription(title: "Metadata Survives")
            made.store.upsertPodcast(sub)
            XCTAssertTrue(made.store.addSubscription(podcastID: sub.id))
            var settings = made.store.state.settings
            settings.hasCompletedOnboarding = true
            made.store.updateSettings(settings)
        }

        let episodeStoreURL = Persistence.episodeStoreURL(for: sharedFileURL)
        try Data("not sqlite".utf8).write(to: episodeStoreURL, options: [.atomic])

        let reopened = AppStateTestSupport.makeIsolatedStore(fileURL: sharedFileURL, reset: false)
        XCTAssertTrue(reopened.store.state.settings.hasCompletedOnboarding)
        // `AppState.init(from:)` inserts the built-in `Podcast.unknown`
        // when missing, so filter it out before the exact-match assertion —
        // only the user's real podcast metadata is what we care about
        // surviving a corrupted SQLite sidecar.
        let userPodcastTitles = reopened.store.state.podcasts
            .filter { $0.id != Podcast.unknownID }
            .map(\.title)
        XCTAssertEqual(userPodcastTitles, ["Metadata Survives"])
    }

    private func makeSubscription(
        feedURL: URL = URL(string: "https://example.com/\(UUID().uuidString).xml")!,
        title: String = "Test Show"
    ) -> Podcast {
        Podcast(feedURL: feedURL, title: title)
    }

    private func makeEpisode(podcastID: UUID, guid: String) -> Episode {
        Episode(
            podcastID: podcastID,
            guid: guid,
            title: "Episode \(guid)",
            pubDate: Date(),
            enclosureURL: URL(string: "https://example.com/\(guid).mp3")!
        )
    }
}
