import Foundation
import XCTest
@testable import Podcastr

@MainActor
final class AgentGeneratedPodcastServiceTests: XCTestCase {
    func testPublishEpisodeUsesLocalEnclosureWithoutLegacyDownloadEvidence() async throws {
        let made = AppStateTestSupport.makeIsolatedStore()
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let audioURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("agent-audio-\(UUID().uuidString).m4a")
        defer { try? FileManager.default.removeItem(at: audioURL) }
        let audio = Data("generated audio".utf8)
        try audio.write(to: audioURL)

        let episode = try await AgentGeneratedPodcastService.publishEpisode(
            title: "Generated",
            description: "Verified",
            audioURL: audioURL,
            durationSeconds: 1,
            in: made.store
        )

        XCTAssertEqual(episode.enclosureURL, audioURL)
        XCTAssertEqual(try Data(contentsOf: episode.enclosureURL), audio)
        XCTAssertEqual(episode.downloadState, .notDownloaded)
        let artifact = try ArtifactRepository(
            fileURL: made.store.persistence.episodeStore.fileURL
        ).current(kind: .downloadFile, subjectID: episode.id)
        XCTAssertNil(artifact)
        XCTAssertEqual(made.store.podcast(id: episode.podcastID)?.kind, .synthetic)
        XCTAssertEqual(episode.description, "Verified")

        let relaunched = AppStateStore(
            persistence: made.store.persistence,
            sharedFeedHost: QueuedCoreFeedHost([]),
            startSubscriptionRefresh: false
        )
        XCTAssertEqual(relaunched.episode(id: episode.id)?.description, "Verified")
        XCTAssertEqual(relaunched.episode(id: episode.id)?.enclosureURL, audioURL)
        XCTAssertEqual(relaunched.episode(id: episode.id)?.downloadState, .notDownloaded)
    }

    func testPublishEpisodeWithMissingFileDoesNotManufactureDownloadedState() async throws {
        let made = AppStateTestSupport.makeIsolatedStore()
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let missingURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("missing-agent-audio-\(UUID().uuidString).m4a")

        let episode = try await AgentGeneratedPodcastService.publishEpisode(
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

    func testAgentOwnedPodcastLifecycleIsRustAuthoritative() async throws {
        let made = AppStateTestSupport.makeIsolatedStore()
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let manager = LiveAgentOwnedPodcastManager(store: made.store)

        let created = try await manager.createPodcast(
            title: "Daily Brief",
            description: "A private briefing.",
            author: "Pod0",
            imageURL: nil,
            language: "en",
            categories: ["News"]
        )
        let podcastID = try XCTUnwrap(UUID(uuidString: created.podcastID))
        XCTAssertEqual(made.store.podcast(id: podcastID)?.title, "Daily Brief")

        let updated = try await manager.updatePodcast(
            podcastID: created.podcastID,
            title: "Evening Brief",
            description: "Updated",
            author: nil,
            imageURL: nil
        )
        XCTAssertEqual(updated.title, "Evening Brief")
        XCTAssertEqual(made.store.podcast(id: podcastID)?.description, "Updated")

        try await manager.deletePodcast(podcastID: created.podcastID)
        XCTAssertNil(made.store.podcast(id: podcastID))
        let relaunched = AppStateStore(
            persistence: made.store.persistence,
            sharedFeedHost: QueuedCoreFeedHost([]),
            startSubscriptionRefresh: false
        )
        XCTAssertNil(relaunched.podcast(id: podcastID))
    }

    func testGeneratedTranscriptCanAdoptTheCoreEpisodeIdentity() {
        let originalID = UUID()
        let coreID = UUID()
        let transcript = Transcript(
            episodeID: originalID,
            language: "en",
            source: .onDevice,
            segments: [Segment(start: 0, end: 1, text: "Hello")]
        )

        let reassigned = transcript.replacingEpisodeID(with: coreID)

        XCTAssertEqual(reassigned.id, transcript.id)
        XCTAssertEqual(reassigned.episodeID, coreID)
        XCTAssertEqual(reassigned.segments, transcript.segments)
    }
}
