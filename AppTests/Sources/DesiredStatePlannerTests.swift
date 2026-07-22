import Foundation
import XCTest
@testable import Podcastr

final class DesiredStatePlannerTests: XCTestCase {
    func testPlanIsPureIdempotentAndVersionDriven() throws {
        let episode = makeEpisode()
        let settings = Settings()
        let planner = DesiredStatePlanner()
        let input = DesiredStatePlanner.Input(
            episodes: [episode], settings: settings, artifacts: [], transcripts: [],
            transcriptDesiredEpisodeIDs: [episode.id], embeddingSpaceID: "space-a",
            scheduledTasks: [], now: Date()
        )

        let first = planner.plan(input)
        XCTAssertEqual(first, planner.plan(input))
        XCTAssertEqual(Set(first.map(\.kind)), [.transcriptIngest])

        let transcript = TranscriptWorkflowSnapshot(
            episodeID: episode.id,
            sourceRevision: DesiredStatePlanner.audioVersion(episode),
            contentDigest: String(repeating: "1", count: 64),
            selectionRevision: 1
        )
        let withTranscript = planner.plan(.init(
            episodes: [episode], settings: settings, artifacts: [], transcripts: [transcript],
            transcriptDesiredEpisodeIDs: [episode.id], embeddingSpaceID: "space-a",
            scheduledTasks: [], now: input.now
        ))
        XCTAssertEqual(Set(withTranscript.map(\.kind)), [.transcriptIndex])

        let indexJob = try XCTUnwrap(withTranscript.first { $0.kind == .transcriptIndex })
        let completeArtifacts = [
            artifact(kind: .semanticIndex, subject: episode.id, input: indexJob.inputVersion),
        ]
        XCTAssertTrue(planner.plan(.init(
            episodes: [episode], settings: settings,
            artifacts: completeArtifacts, transcripts: [transcript],
            transcriptDesiredEpisodeIDs: [episode.id], embeddingSpaceID: "space-a",
            scheduledTasks: [], now: input.now
        )).isEmpty)

        let modelChanged = planner.plan(.init(
            episodes: [episode], settings: settings,
            artifacts: completeArtifacts, transcripts: [transcript],
            transcriptDesiredEpisodeIDs: [episode.id], embeddingSpaceID: "space-b",
            scheduledTasks: [], now: input.now
        ))
        XCTAssertEqual(Set(modelChanged.map(\.kind)), [.transcriptIndex])
    }

    func testPolicyAndInputChangesProduceDeterministicPlanChanges() {
        var episode = makeEpisode()
        let planner = DesiredStatePlanner()
        let settings = Settings()
        let desired = planner.plan(.init(
            episodes: [episode], settings: settings, artifacts: [], transcripts: [],
            transcriptDesiredEpisodeIDs: [episode.id], scheduledTasks: [], now: Date()
        ))
        XCTAssertTrue(desired.contains { $0.kind == .transcriptIngest })

        let disabled = planner.plan(.init(
            episodes: [episode], settings: settings, artifacts: [], transcripts: [],
            transcriptDesiredEpisodeIDs: [], scheduledTasks: [], now: Date()
        ))
        XCTAssertFalse(disabled.contains { $0.kind == .transcriptIngest })

        let oldKey = desired.first { $0.kind == .transcriptIngest }?.idempotencyKey
        episode.enclosureURL = URL(string: "https://example.com/replaced.mp3")!
        let changed = planner.plan(.init(
            episodes: [episode], settings: settings, artifacts: [], transcripts: [],
            transcriptDesiredEpisodeIDs: [episode.id], scheduledTasks: [], now: Date()
        ))
        XCTAssertNotEqual(oldKey, changed.first { $0.kind == .transcriptIngest }?.idempotencyKey)
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
            episodes: [], settings: settings, artifacts: [], transcripts: [],
            transcriptDesiredEpisodeIDs: [], scheduledTasks: [task], now: due
        )).first)
        let firstPayload = try XCTUnwrap(first.payload)

        task.prompt = "Edited prompt"
        task.nextRunAt = due.addingTimeInterval(3_600)
        let edited = try XCTUnwrap(planner.plan(.init(
            episodes: [], settings: settings, artifacts: [], transcripts: [],
            transcriptDesiredEpisodeIDs: [], scheduledTasks: [task],
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
            episodes: [episode], settings: Settings(), artifacts: [], transcripts: [],
            transcriptDesiredEpisodeIDs: [], scheduledTasks: [], now: Date()
        ))
        XCTAssertTrue(jobs.isEmpty)
    }

    func testSwiftPlannerNeverCreatesAnyChapterWorkflow() throws {
        var episode = makeEpisode()
        episode.chaptersURL = URL(string: "https://example.com/chapters.json")!
        let transcript = TranscriptWorkflowSnapshot(
            episodeID: episode.id,
            sourceRevision: DesiredStatePlanner.audioVersion(episode),
            contentDigest: String(repeating: "1", count: 64),
            selectionRevision: 1
        )
        let jobs = DesiredStatePlanner().plan(.init(
            episodes: [episode], settings: Settings(), artifacts: [], transcripts: [transcript],
            transcriptDesiredEpisodeIDs: [], embeddingSpaceID: "space-a",
            scheduledTasks: [], now: Date()
        ))

        XCTAssertEqual(jobs.map(\.kind), [.transcriptIndex])
        XCTAssertFalse(WorkJobKind.allCases.map(\.rawValue).contains("publisherChapters"))
        XCTAssertFalse(WorkJobKind.allCases.map(\.rawValue).contains("chapterArtifacts"))
    }

    private func makeEpisode() -> Episode {
        Episode(
            podcastID: UUID(), guid: "planner", title: "Planner",
            pubDate: Date(), enclosureURL: URL(string: "https://example.com/audio.mp3")!
        )
    }

    private func artifact(
        kind: ArtifactKind,
        subject: UUID,
        input: String,
        output: String = "output"
    ) -> ArtifactRecord {
        ArtifactRecord(
            kind: kind, subjectID: subject, inputVersion: input,
            outputVersion: output, contentHash: output,
            location: nil, origin: "test", schemaVersion: 1,
            integrity: .available, verifiedAt: Date()
        )
    }

}
