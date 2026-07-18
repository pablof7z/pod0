import Foundation
import XCTest
@testable import Podcastr

@MainActor
final class AgentGeneratedPodcastServiceTests: XCTestCase {
    func testPublishEpisodeCommitsVerifiedDownloadEvidence() throws {
        let made = AppStateTestSupport.makeIsolatedStore()
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let audioURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("agent-audio-\(UUID().uuidString).m4a")
        defer { try? FileManager.default.removeItem(at: audioURL) }
        let audio = Data("generated audio".utf8)
        try audio.write(to: audioURL)

        let episode = AgentGeneratedPodcastService.publishEpisode(
            title: "Generated",
            description: "Verified",
            audioURL: audioURL,
            durationSeconds: 1,
            in: made.store
        )

        guard case .downloaded(let selectedURL, let byteCount) = episode.downloadState else {
            return XCTFail("Expected verified downloaded projection")
        }
        XCTAssertEqual(selectedURL, audioURL)
        XCTAssertEqual(byteCount, Int64(audio.count))
        let artifact = try ArtifactRepository(
            fileURL: made.store.persistence.episodeStore.fileURL
        ).current(kind: .downloadFile, subjectID: episode.id)
        XCTAssertEqual(artifact?.integrity, .available)
        XCTAssertEqual(artifact?.contentHash, ArtifactRepository.hash(audio))
    }

    func testPublishEpisodeWithMissingFileDoesNotManufactureDownloadedState() throws {
        let made = AppStateTestSupport.makeIsolatedStore()
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let missingURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("missing-agent-audio-\(UUID().uuidString).m4a")

        let episode = AgentGeneratedPodcastService.publishEpisode(
            title: "Missing",
            description: "No evidence",
            audioURL: missingURL,
            durationSeconds: nil,
            in: made.store
        )

        XCTAssertEqual(episode.downloadState, .notDownloaded)
        let artifact = try ArtifactRepository(
            fileURL: made.store.persistence.episodeStore.fileURL
        ).current(kind: .downloadFile, subjectID: episode.id)
        XCTAssertNil(artifact)
    }
}
