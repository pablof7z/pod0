import Foundation
import XCTest
@testable import Podcastr

final class DesiredStatePlannerTests: XCTestCase {
    func testPlanIsPureAndDoesNotCreateMigratedWorkflowKinds() {
        let planner = DesiredStatePlanner()
        let input = DesiredStatePlanner.Input(
            settings: Settings(),
            scheduledTasks: [], now: Date()
        )

        let first = planner.plan(input)
        XCTAssertEqual(first, planner.plan(input))
        XCTAssertTrue(first.isEmpty)
        XCTAssertFalse(JobStore.supportedKindSQL.contains("transcriptIngest"))
        XCTAssertFalse(JobStore.supportedKindSQL.contains("transcriptIndex"))
    }

    func testAudioVersionRemainsDeterministicForLegacyCutover() {
        var episode = makeEpisode()
        let initial = DesiredStatePlanner.audioVersion(episode)
        XCTAssertEqual(initial, DesiredStatePlanner.audioVersion(episode))
        episode.enclosureURL = URL(string: "https://example.com/replaced.mp3")!
        XCTAssertNotEqual(initial, DesiredStatePlanner.audioVersion(episode))
    }

    func testScheduledOccurrenceIdentityAndPayloadAreImmutable() throws {
        let due = Date(timeIntervalSince1970: 10_000)
        var task = AgentScheduledTask(
            id: UUID(), label: "Daily brief", prompt: "Original prompt",
            intervalSeconds: 3_600, createdAt: due.addingTimeInterval(-100),
            lastRunAt: nil, nextRunAt: due
        )
        let settings = Settings()
        let planner = DesiredStatePlanner()
        let first = try XCTUnwrap(planner.plan(.init(
            settings: settings, scheduledTasks: [task], now: due
        )).first)
        let firstPayload = try XCTUnwrap(first.payload)

        task.prompt = "Edited prompt"
        task.nextRunAt = due.addingTimeInterval(3_600)
        let edited = try XCTUnwrap(planner.plan(.init(
            settings: settings, scheduledTasks: [task],
            now: due.addingTimeInterval(3_600)
        )).first)

        XCTAssertEqual(
            first.idempotencyKey,
            DesiredStatePlanner.scheduledOccurrenceID(taskID: task.id, scheduledFor: due)
        )
        XCTAssertNotEqual(first.idempotencyKey, edited.idempotencyKey)
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        XCTAssertEqual(
            try decoder.decode(ScheduledRunPayload.self, from: firstPayload).prompt,
            "Original prompt"
        )
    }

    func testPlannerNeverCreatesSwiftPublisherChapterWork() {
        var episode = makeEpisode()
        episode.chaptersURL = URL(string: "https://example.com/chapters-v1.json")!
        let jobs = DesiredStatePlanner().plan(.init(
            settings: Settings(), scheduledTasks: [], now: Date()
        ))
        XCTAssertTrue(jobs.isEmpty)
    }

    func testSwiftPlannerNeverCreatesAnyChapterWorkflow() throws {
        var episode = makeEpisode()
        episode.chaptersURL = URL(string: "https://example.com/chapters.json")!
        let jobs = DesiredStatePlanner().plan(.init(
            settings: Settings(), scheduledTasks: [], now: Date()
        ))

        XCTAssertTrue(jobs.isEmpty)
        XCTAssertFalse(WorkJobKind.allCases.map(\.rawValue).contains("publisherChapters"))
        XCTAssertFalse(WorkJobKind.allCases.map(\.rawValue).contains("chapterArtifacts"))
    }

    private func makeEpisode() -> Episode {
        Episode(
            podcastID: UUID(), guid: "planner", title: "Planner",
            pubDate: Date(), enclosureURL: URL(string: "https://example.com/audio.mp3")!
        )
    }

}
