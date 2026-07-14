import XCTest
@testable import Podcastr

/// Coverage for the `markEpisodePlayed` / `markEpisodeUnplayed` and
/// `setEpisodePlaybackPosition` state transitions.
///
/// These methods drive the Now Playing menu's Mark Played / Mark Unplayed
/// actions and the Home episode-row swipe gestures. The transitions have
/// non-obvious side effects (mark-played zeroes the playback position so a
/// re-play starts from the top) so they deserve regression tests.
@MainActor
final class EpisodePlayedStateTests: XCTestCase {

    private var fileURL: URL!
    private var store: AppStateStore!

    override func setUp() async throws {
        try await super.setUp()
        let made = AppStateTestSupport.makeIsolatedStore()
        fileURL = made.fileURL
        store = made.store
    }

    override func tearDown() async throws {
        if let fileURL {
            AppStateTestSupport.disposeIsolatedStore(at: fileURL)
        }
        store = nil
        fileURL = nil
        try await super.tearDown()
    }

    // MARK: - markEpisodePlayed

    func testMarkPlayedFlipsTheFlag() throws {
        let (sub, ep) = seed()

        store.markEpisodePlayed(ep.id)

        let updated = try XCTUnwrap(store.state.episodes.first { $0.id == ep.id })
        XCTAssertTrue(updated.played, "Mark-played must set `played = true`")
        XCTAssertEqual(updated.podcastID, sub.id)
    }

    func testMarkPlayedResetsPlaybackPosition() throws {
        // Half-listened episode → mark-played → re-play should start from 0.
        let (_, ep) = seed()
        store.setEpisodePlaybackPosition(ep.id, position: 1234.5)
        XCTAssertEqual(store.state.episodes.first?.playbackPosition, 1234.5)

        store.markEpisodePlayed(ep.id)

        let updated = try XCTUnwrap(store.state.episodes.first { $0.id == ep.id })
        XCTAssertEqual(updated.playbackPosition, 0,
                       "Mark-played must reset playback to 0 so re-play starts from the top")
    }

    func testMarkPlayedNoOpsForUnknownID() {
        let (_, ep) = seed()
        let snapshot = store.state.episodes.first { $0.id == ep.id }

        store.markEpisodePlayed(UUID())

        let after = store.state.episodes.first { $0.id == ep.id }
        XCTAssertEqual(snapshot, after, "Mark-played with an unknown ID must not mutate any episode")
    }

    // MARK: - markEpisodeUnplayed

    func testMarkUnplayedClearsTheFlag() throws {
        let (_, ep) = seed()
        store.markEpisodePlayed(ep.id)
        XCTAssertEqual(store.state.episodes.first?.played, true)

        store.markEpisodeUnplayed(ep.id)

        let updated = try XCTUnwrap(store.state.episodes.first { $0.id == ep.id })
        XCTAssertFalse(updated.played)
    }

    func testMarkUnplayedDoesNotReviveAPosition() throws {
        // mark-played zeroes position. mark-unplayed should NOT restore the
        // pre-mark-played position; it only flips the flag. The user is
        // saying "I haven't listened" — re-listening from 0 is correct.
        let (_, ep) = seed()
        store.setEpisodePlaybackPosition(ep.id, position: 1234.5)
        store.markEpisodePlayed(ep.id)

        store.markEpisodeUnplayed(ep.id)

        let updated = try XCTUnwrap(store.state.episodes.first { $0.id == ep.id })
        XCTAssertEqual(updated.playbackPosition, 0)
        XCTAssertFalse(updated.played)
    }

    // MARK: - Round-trip

    func testRoundTrip() throws {
        let (_, ep) = seed()
        XCTAssertFalse(store.state.episodes.first?.played ?? true)

        store.markEpisodePlayed(ep.id)
        XCTAssertEqual(store.state.episodes.first?.played, true)

        store.markEpisodeUnplayed(ep.id)
        XCTAssertEqual(store.state.episodes.first?.played, false)

        store.markEpisodePlayed(ep.id)
        XCTAssertEqual(store.state.episodes.first?.played, true)
    }

    // MARK: - setEpisodePlaybackPosition

    func testSetPlaybackPositionPersistsToTheRightEpisode() throws {
        let (sub, _) = seed()
        let other = makeEpisode(podcastID: sub.id, guid: "other-\(UUID().uuidString)")
        store.upsertEpisodes([other], forPodcast: sub.id)

        store.setEpisodePlaybackPosition(other.id, position: 42)

        let otherStored = try XCTUnwrap(store.state.episodes.first { $0.id == other.id })
        XCTAssertEqual(otherStored.playbackPosition, 42, accuracy: 0.001)
        // Other episodes in the same subscription are untouched.
        let originalsUntouched = store.state.episodes.filter { $0.id != other.id }
        XCTAssertTrue(originalsUntouched.allSatisfy { $0.playbackPosition == 0 })
    }

    // MARK: - Fixtures

    /// Adds one subscription and one episode with `played == false`,
    /// returns both.
    @discardableResult
    private func seed() -> (Podcast, Episode) {
        let sub = Podcast(
            feedURL: URL(string: "https://example.com/\(UUID().uuidString).xml")!,
            title: "Played-State Test Show"
        )
        store.upsertPodcast(sub)
        store.addSubscription(podcastID: sub.id)
        let ep = makeEpisode(podcastID: sub.id, guid: "seed-\(UUID().uuidString)")
        store.upsertEpisodes([ep], forPodcast: sub.id)
        return (sub, ep)
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
