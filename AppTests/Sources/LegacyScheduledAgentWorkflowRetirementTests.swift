import Foundation
import XCTest
@testable import Podcastr

@MainActor
final class LegacyScheduledAgentWorkflowRetirementTests: XCTestCase {
    func testExactSourceRetiresAtomicallyAndRepeatedRestartIsNoOp() throws {
        let made = makeSource()
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let historyURL = made.fileURL.appendingPathExtension("history.json")
        defer { try? FileManager.default.removeItem(at: historyURL) }
        let history = ChatHistoryStore(fileURL: historyURL)
        let snapshot = try LegacyScheduledAgentWorkflowSnapshot.capture(
            state: made.state,
            jobStore: made.jobStore,
            history: history
        )

        XCTAssertTrue(try made.persistence.retireLegacyScheduledAgentSource(
            state: made.state,
            matching: snapshot.backup
        ))
        let retiredState = try made.persistence.load()
        XCTAssertTrue(retiredState.agentScheduledTasks.isEmpty)
        XCTAssertTrue(try made.jobStore.legacyScheduledAgentSourceIsRetired())
        XCTAssertTrue(try made.persistence.retireLegacyScheduledAgentSource(
            state: retiredState,
            matching: snapshot.backup
        ))
    }

    func testSourceChangeRejectsRetirementWithoutDeletingEvidence() throws {
        let made = makeSource()
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let historyURL = made.fileURL.appendingPathExtension("history.json")
        defer { try? FileManager.default.removeItem(at: historyURL) }
        let snapshot = try LegacyScheduledAgentWorkflowSnapshot.capture(
            state: made.state,
            jobStore: made.jobStore,
            history: ChatHistoryStore(fileURL: historyURL)
        )
        _ = try made.jobStore.ensureJob(DesiredJob(
            idempotencyKey: "late-scheduled-source",
            kind: .scheduledAgentRun,
            subjectID: LegacyScheduledAgentWorkflowTestSupport.taskID,
            inputVersion: "late",
            occurrenceID: "late-scheduled-source",
            resourceClass: .scheduledAgent
        ))

        XCTAssertFalse(try made.persistence.retireLegacyScheduledAgentSource(
            state: made.state,
            matching: snapshot.backup
        ))
        XCTAssertEqual(try made.jobStore.legacyScheduledAgentJobs().count, 2)
        XCTAssertEqual(try made.persistence.load().agentScheduledTasks.count, 1)
    }

    private func makeSource() -> (
        fileURL: URL,
        persistence: Persistence,
        state: AppState,
        jobStore: JobStore
    ) {
        let fileURL = AppStateTestSupport.uniqueTempFileURL()
        let persistence = Persistence(fileURL: fileURL)
        var state = AppState()
        let task = LegacyScheduledAgentWorkflowTestSupport.task()
        state.agentScheduledTasks = [task]
        XCTAssertTrue(persistence.write(state, revision: 1))
        state = try! persistence.load()
        let jobStore = JobStore(fileURL: persistence.episodeStore.fileURL)
        let row = LegacyScheduledAgentWorkflowTestSupport.job(
            scheduledFor: task.nextRunAt,
            state: .pending,
            attempt: 0
        )
        _ = try! jobStore.ensureJob(DesiredJob(
            idempotencyKey: row.idempotencyKey,
            kind: row.kind,
            subjectID: row.subjectID,
            inputVersion: row.inputVersion,
            occurrenceID: row.occurrenceID,
            payloadVersion: row.payloadVersion,
            payload: row.payload,
            priority: row.priority,
            resourceClass: row.resourceClass,
            maxAttempts: row.maxAttempts
        ))
        let artifact = ArtifactRecord(
            kind: .scheduledOutput,
            subjectID: task.id,
            inputVersion: "old",
            outputVersion: "old",
            contentHash: ArtifactRepository.hash(Data("old".utf8)),
            location: nil,
            origin: "legacy",
            schemaVersion: 1,
            integrity: .available,
            verifiedAt: LegacyScheduledAgentWorkflowTestSupport.baseDate
        )
        try! ArtifactRepository(fileURL: persistence.episodeStore.fileURL).adopt(artifact)
        return (fileURL, persistence, state, jobStore)
    }
}
