import Foundation
import XCTest
@testable import Podcastr

final class DesiredStatePlannerTests: XCTestCase {
    func testLegacyIdentitiesRemainDeterministicWithoutSwiftWorkflowPlanning() {
        var episode = makeEpisode()
        let initial = DesiredStatePlanner.audioVersion(episode)
        XCTAssertEqual(initial, DesiredStatePlanner.audioVersion(episode))
        episode.enclosureURL = URL(string: "https://example.com/replaced.mp3")!
        XCTAssertNotEqual(initial, DesiredStatePlanner.audioVersion(episode))

        let taskID = UUID()
        let due = Date(timeIntervalSince1970: 10_000)
        XCTAssertEqual(
            DesiredStatePlanner.scheduledOccurrenceID(taskID: taskID, scheduledFor: due),
            "scheduled:\(taskID.uuidString):10000"
        )
    }

    func testSwiftCoordinatorCannotClaimMigratedWorkflowKinds() {
        XCTAssertFalse(JobStore.supportedKindSQL.contains("download"))
        XCTAssertFalse(JobStore.supportedKindSQL.contains("transcriptIngest"))
        XCTAssertFalse(JobStore.supportedKindSQL.contains("transcriptIndex"))
        XCTAssertFalse(JobStore.supportedKindSQL.contains("scheduledAgentRun"))
    }

    private func makeEpisode() -> Episode {
        Episode(
            podcastID: UUID(), guid: "planner", title: "Planner",
            pubDate: Date(), enclosureURL: URL(string: "https://example.com/audio.mp3")!
        )
    }
}
