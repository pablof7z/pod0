import XCTest
@testable import Podcastr

@MainActor
final class PlaybackResumeDurabilityTests: XCTestCase {
    func testSuspensionBoundaryPersistsEpisodeIdentityAndLatestPosition() async throws {
        let made = AppStateTestSupport.makeIsolatedStore()
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let podcast = Podcast(
            feedURL: URL(string: "https://example.com/qualification.xml")!,
            title: "Qualification Show"
        )
        let episode = Episode(
            podcastID: podcast.id,
            guid: "resume-\(UUID().uuidString)",
            title: "Resume Qualification",
            pubDate: Date(),
            duration: 1_800,
            enclosureURL: URL(string: "https://example.com/resume.mp3")!
        )
        made.store.upsertPodcast(podcast)
        _ = made.store.addSubscription(podcastID: podcast.id)
        made.store.upsertEpisodes([episode], forPodcast: podcast.id)
        made.store.setLastPlayedEpisode(episode.id)
        made.store.setEpisodePlaybackPosition(episode.id, position: 1)
        made.store.setEpisodePlaybackPosition(episode.id, position: 127.5)

        await made.store.flushForSuspension()

        let reopened = AppStateTestSupport.makeIsolatedStore(
            fileURL: made.fileURL,
            reset: false
        )
        XCTAssertEqual(reopened.store.state.lastPlayedEpisodeID, episode.id)
        XCTAssertEqual(
            reopened.store.episode(id: episode.id)?.playbackPosition ?? -1,
            127.5,
            accuracy: 0.001
        )
    }
}
