import XCTest
@testable import Podcastr

/// Coverage for lock-screen / Control Center Play after a cold restart.
///
/// The remote command path must stage the restored episode in Rust before
/// starting audio when the kernel has no now-playing episode yet.
@MainActor
final class PlaybackRemoteCommandRoutingTests: XCTestCase {

    func testRemotePlayStagesRestoredEpisodeWhenKernelHasNoNowPlaying() {
        let restoredEpisodeID = UUID()

        XCTAssertEqual(
            PlaybackState.restoredEpisodeIDToStageBeforeRemotePlay(
                kernelNowPlayingEpisodeID: nil,
                restoredEpisodeID: restoredEpisodeID
            ),
            restoredEpisodeID
        )

        XCTAssertEqual(
            PlaybackState.restoredEpisodeIDToStageBeforeRemotePlay(
                kernelNowPlayingEpisodeID: "",
                restoredEpisodeID: restoredEpisodeID
            ),
            restoredEpisodeID
        )
    }

    func testRemotePlayTreatsWhitespaceKernelEpisodeIDAsMissing() {
        let restoredEpisodeID = UUID()

        XCTAssertEqual(
            PlaybackState.restoredEpisodeIDToStageBeforeRemotePlay(
                kernelNowPlayingEpisodeID: " \n\t ",
                restoredEpisodeID: restoredEpisodeID
            ),
            restoredEpisodeID
        )
    }

    func testRemotePlayDoesNotRestageWhenKernelAlreadyHasNowPlaying() {
        XCTAssertNil(
            PlaybackState.restoredEpisodeIDToStageBeforeRemotePlay(
                kernelNowPlayingEpisodeID: UUID().uuidString,
                restoredEpisodeID: UUID()
            )
        )
    }

    func testRemotePlayCannotStageWithoutRestoredEpisode() {
        XCTAssertNil(
            PlaybackState.restoredEpisodeIDToStageBeforeRemotePlay(
                kernelNowPlayingEpisodeID: nil,
                restoredEpisodeID: nil
            )
        )
    }

    func testRemotePlayDoesNotRestageWhenLocalLoadAlreadyArrived() {
        let episodeID = UUID()

        XCTAssertNil(
            PlaybackState.restoredEpisodeIDToStageBeforeRemotePlay(
                kernelNowPlayingEpisodeID: nil,
                restoredEpisodeID: episodeID,
                locallyLoadedEpisodeID: episodeID
            )
        )
    }
}
