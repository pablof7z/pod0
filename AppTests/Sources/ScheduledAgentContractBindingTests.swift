import Pod0Core
import XCTest

final class ScheduledAgentContractBindingTests: XCTestCase {
    func testSwiftQualifiesScheduledAgentCommandAndHostContract() {
        let digest = ContentDigest(word0: 1, word1: 2, word2: 3, word3: 4)
        let task = ScheduledTaskInput(
            taskId: ScheduledTaskId(high: 5, low: 6),
            label: "Daily briefing",
            prompt: "Prepare a daily briefing",
            modelReference: "openrouter:test/model",
            intervalMilliseconds: 86_400_000,
            nextRunAt: UnixTimestampMilliseconds(value: 1_000)
        )
        let command = ApplicationCommand.ensureScheduledTask(task: task)
        guard case let .ensureScheduledTask(decodedTask) = command else {
            return XCTFail("Expected a typed scheduled-task command")
        }
        XCTAssertEqual(decodedTask, task)

        let occurrenceID = ScheduledOccurrenceId(high: 7, low: 8)
        let attemptID = ScheduledAttemptId(high: 9, low: 10)
        let execution = ScheduledAgentExecutionRequest(
            occurrenceId: occurrenceID,
            attemptId: attemptID,
            promptRevision: digest,
            prompt: task.prompt,
            modelReference: task.modelReference,
            context: [
                ScheduledAgentContextMessage(role: .user, content: "Use saved podcast evidence")
            ],
            maximumOutputBytes: 16_384
        )
        XCTAssertEqual(
            HostRequest.executeScheduledAgentTurn(execution: execution),
            .executeScheduledAgentTurn(execution: execution)
        )

        let completed = ScheduledAgentExecutionObservation.completed(
            occurrenceId: occurrenceID,
            attemptId: attemptID,
            artifactId: GeneratedArtifactId(high: 11, low: 12),
            outputDigest: digest,
            outputExcerpt: "Briefing ready"
        )
        XCTAssertEqual(
            HostObservation.scheduledAgentExecutionObserved(observation: completed),
            .scheduledAgentExecutionObserved(observation: completed)
        )
    }

    func testSwiftScheduledAgentProjectionPreservesSingleWriterGuard() {
        let facade = Pod0Facade()
        let envelope = facade.snapshot(
            request: ProjectionRequest(
                scope: .scheduledAgent(taskId: nil),
                offset: 0,
                maxItems: 20
            )
        )

        XCTAssertEqual(envelope.contractVersion, 36)
        guard case let .scheduledAgent(projection) = envelope.projection else {
            return XCTFail("Expected a scheduled-agent projection")
        }
        XCTAssertTrue(projection.tasks.isEmpty)
        XCTAssertTrue(projection.workflows.isEmpty)
        XCTAssertEqual(projection.failure?.code, .storageUnavailable)
    }
}
