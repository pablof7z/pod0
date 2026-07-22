import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class LegacyScheduledAgentWorkflowCutoverTests: XCTestCase {
    func testBootstrapImportsPendingRunRetiresSwiftAuthorityAndSurvivesRestart() throws {
        let fixture = SharedTranscriptRecoveryTestSupport.makeFixture()
        defer { SharedTranscriptRecoveryTestSupport.dispose(fixture) }
        var state = try fixture.persistence.load()
        let task = LegacyScheduledAgentWorkflowTestSupport.task()
        state.agentScheduledTasks = [task]
        _ = fixture.persistence.save(state)

        let scheduledFor = task.nextRunAt
        let occurrence = DesiredStatePlanner.scheduledOccurrenceID(
            taskID: task.id,
            scheduledFor: scheduledFor
        )
        let payload = ScheduledRunPayload(
            taskID: task.id,
            scheduledFor: scheduledFor,
            prompt: task.prompt,
            modelID: state.settings.agentInitialModel,
            intervalSeconds: task.intervalSeconds
        )
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        let store = JobStore(fileURL: fixture.persistence.episodeStore.fileURL)
        _ = try store.ensureJob(DesiredJob(
            idempotencyKey: occurrence,
            kind: .scheduledAgentRun,
            subjectID: task.id,
            inputVersion: occurrence,
            occurrenceID: occurrence,
            payload: try encoder.encode(payload),
            priority: 60,
            resourceClass: .scheduledAgent,
            maxAttempts: 12
        ))

        let client = try bootstrap(fixture)
        let report = client.facade.scheduledAgentCutover()
        XCTAssertEqual(report.stage, .authoritative)
        XCTAssertEqual(report.taskCount, 1)
        XCTAssertEqual(report.occurrenceCount, 1)
        let generation = try XCTUnwrap(report.sourceGeneration)
        let backup = try LegacyScheduledAgentWorkflowBackup.load(
            from: fixture.persistence.legacyScheduledAgentWorkflowBackupRootURL,
            sourceGeneration: generation,
            expectedDigest: report.backupDigest,
            expectedByteCount: report.backupByteCount
        )
        XCTAssertEqual(backup.tasks, [task])
        XCTAssertEqual(backup.jobs.count, 1)
        XCTAssertTrue(try store.legacyScheduledAgentSourceIsRetired())
        XCTAssertTrue(try fixture.persistence.load().agentScheduledTasks.isEmpty)
        let projection = scheduledProjection(client.facade)
        XCTAssertEqual(projection.tasks.first?.taskId.uuid, task.id)
        XCTAssertEqual(projection.workflows.first?.taskId.uuid, task.id)
        client.shutdown()

        let reopened = try Pod0Facade.open(
            storePath: fixture.persistence.sharedCoreStoreURL.path
        )
        XCTAssertEqual(reopened.scheduledAgentCutover().stage, .authoritative)
        XCTAssertEqual(scheduledProjection(reopened).tasks.first?.taskId.uuid, task.id)
    }

    func testEmptyLegacySourceStillCommitsExplicitRustAuthority() throws {
        let fixture = SharedTranscriptRecoveryTestSupport.makeFixture()
        defer { SharedTranscriptRecoveryTestSupport.dispose(fixture) }
        let client = try bootstrap(fixture)
        defer { client.shutdown() }
        let report = client.facade.scheduledAgentCutover()
        XCTAssertEqual(report.stage, .authoritative)
        XCTAssertEqual(report.taskCount, 0)
        XCTAssertEqual(report.occurrenceCount, 0)
        XCTAssertNotNil(report.backupDigest)
    }

    func testNativeAdapterCreatesUpdatesAndRemovesThroughTypedRustCommands() throws {
        let fixture = SharedTranscriptRecoveryTestSupport.makeFixture()
        defer { SharedTranscriptRecoveryTestSupport.dispose(fixture) }
        let client = try bootstrap(fixture)
        defer { client.shutdown() }
        let taskID = UUID()
        let nextRun = Date().addingTimeInterval(86_400)
        XCTAssertTrue(client.ensureScheduledTask(
            id: taskID,
            label: "Morning brief",
            prompt: "Summarize highlights",
            intervalSeconds: 86_400,
            modelReference: "openrouter:test/model",
            nextRunAt: nextRun
        ))
        let created = try XCTUnwrap(scheduledProjection(client.facade).tasks.first {
            $0.taskId.uuid == taskID
        })
        XCTAssertEqual(created.label, "Morning brief")
        XCTAssertTrue(client.updateScheduledTask(
            id: taskID,
            label: "Updated brief",
            prompt: "Summarize notes",
            intervalSeconds: 43_200,
            modelReference: "openrouter:test/model",
            nextRunAt: nextRun.addingTimeInterval(10)
        ))
        XCTAssertEqual(
            scheduledProjection(client.facade).tasks.first { $0.taskId.uuid == taskID }?.label,
            "Updated brief"
        )
        XCTAssertTrue(client.removeScheduledTask(id: taskID))
        XCTAssertNil(scheduledProjection(client.facade).tasks.first { $0.taskId.uuid == taskID })
    }

    private func bootstrap(
        _ fixture: SharedTranscriptRecoveryTestSupport.Fixture
    ) throws -> SharedLibraryClient {
        switch SharedLibraryBootstrap.run(
            persistence: fixture.persistence,
            legacyState: try fixture.persistence.load(),
            feedHost: QueuedCoreFeedHost([])
        ) {
        case .ready(let client): client
        case .authoritativeUnavailable(let reason, let stage):
            throw Failure.bootstrap("\(stage.rawValue):\(reason)")
        }
    }

    private func scheduledProjection(_ facade: Pod0Facade) -> ScheduledAgentProjection {
        let envelope = facade.snapshot(request: ProjectionRequest(
            scope: .scheduledAgent(taskId: nil),
            offset: 0,
            maxItems: 20
        ))
        guard case .scheduledAgent(let projection) = envelope.projection else {
            return ScheduledAgentProjection(
                tasks: [], workflows: [], hasMore: false, failure: nil
            )
        }
        return projection
    }

    private enum Failure: Error { case bootstrap(String) }
}
