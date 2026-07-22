import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class Pod0NativeHostDispatcherScheduledAgentTests: XCTestCase {
    func testDuplicateDispatchStartsOneProviderTurnAndEmitsOrderedEvidence() async throws {
        let output = "Briefing complete"
        let host = ScheduledAgentHostStub(result: qualifyScheduledAgentCompletion(
            execution: execution(),
            rawOutput: output
        )!)
        let made = try makeDispatcher(host: host)
        defer { made.cleanup() }
        let request = envelope()
        var observations: [HostObservationEnvelope] = []

        made.dispatcher.execute(request) { observations.append($0) }
        made.dispatcher.execute(request) { observations.append($0) }
        let task = try XCTUnwrap(made.dispatcher.activeTasks[request.requestId]?.task)
        await task.value

        XCTAssertEqual(host.executionCount, 1)
        XCTAssertEqual(observations.map(\.sequenceNumber), [0, 1])
        guard case .scheduledAgentExecutionObserved(.accepted) = observations.first?.observation,
              case .scheduledAgentExecutionObserved(.completed) = observations.last?.observation
        else { return XCTFail("Expected accepted then completed") }
    }

    func testCancellationIsIdempotentAndSuppressesLateProviderCallback() async throws {
        let host = SuspendedScheduledAgentHost()
        let made = try makeDispatcher(host: host)
        defer { made.cleanup() }
        let request = envelope()
        var observations: [HostObservationEnvelope] = []

        made.dispatcher.execute(request) { observations.append($0) }
        await Task.yield()
        made.dispatcher.cancel(
            requestID: request.requestId,
            cancellationID: request.cancellationId
        )
        made.dispatcher.cancel(
            requestID: request.requestId,
            cancellationID: request.cancellationId
        )
        host.complete(with: qualifyScheduledAgentCompletion(
            execution: execution(),
            rawOutput: "Late completion"
        )!)
        await Task.yield()
        await Task.yield()

        XCTAssertEqual(observations.map(\.sequenceNumber), [0, 1])
        guard case .scheduledAgentExecutionObserved(.cancelled) = observations.last?.observation
        else { return XCTFail("Expected cancellation") }
        XCTAssertTrue(made.dispatcher.activeTasks.isEmpty)
    }

    func testExpiredRequestNeverStartsProviderAndReturnsCorrelatedFailure() async throws {
        let host = ScheduledAgentHostStub(result: .unsupported(wireCode: 1))
        let made = try makeDispatcher(
            host: host,
            now: { Date(timeIntervalSince1970: 10) }
        )
        defer { made.cleanup() }
        var request = envelope()
        request = HostRequestEnvelope(
            requestId: request.requestId,
            commandId: request.commandId,
            cancellationId: request.cancellationId,
            issuedRevision: request.issuedRevision,
            deadlineAt: UnixTimestampMilliseconds(value: 9_000),
            request: request.request
        )
        var observations: [HostObservationEnvelope] = []

        made.dispatcher.execute(request) { observations.append($0) }

        XCTAssertEqual(host.executionCount, 0)
        guard case let .scheduledAgentExecutionObserved(.failed(
            occurrenceID, attemptID, code, _, _
        )) = observations.first?.observation else {
            return XCTFail("Expected correlated expiry")
        }
        XCTAssertEqual(occurrenceID, execution().occurrenceId)
        XCTAssertEqual(attemptID, execution().attemptId)
        XCTAssertEqual(code, .network)
    }

    private func makeDispatcher(
        host: any CoreScheduledAgentHosting,
        now: @escaping @MainActor () -> Date = Date.init
    ) throws -> (dispatcher: Pod0NativeHostDispatcher, cleanup: () -> Void) {
        let outboxURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("scheduled-agent-outbox-\(UUID().uuidString).json")
        let outbox = try NativeHostObservationOutbox(fileURL: outboxURL)
        let dispatcher = Pod0NativeHostDispatcher(
            feedHost: ScheduledAgentUnusedFeedHost(),
            playbackHost: ScheduledAgentUnusedPlaybackHost(),
            scheduledAgentHost: host,
            now: now,
            observationOutbox: outbox
        )
        return (dispatcher, { try? FileManager.default.removeItem(at: outboxURL) })
    }

    private func envelope() -> HostRequestEnvelope {
        HostRequestEnvelope(
            requestId: HostRequestId(high: 9, low: 10),
            commandId: CommandId(high: 11, low: 12),
            cancellationId: CancellationId(high: 13, low: 14),
            issuedRevision: StateRevision(value: 15),
            deadlineAt: nil,
            request: .executeScheduledAgentTurn(execution: execution())
        )
    }

    private func execution() -> ScheduledAgentExecutionRequest {
        ScheduledAgentExecutionRequest(
            occurrenceId: ScheduledOccurrenceId(high: 1, low: 2),
            attemptId: ScheduledAttemptId(high: 3, low: 4),
            promptRevision: ContentDigest(word0: 5, word1: 6, word2: 7, word3: 8),
            prompt: "Prepare my briefing",
            modelReference: "openrouter:test/model",
            context: [],
            maximumOutputBytes: 16_384
        )
    }
}

@MainActor
private final class ScheduledAgentHostStub: CoreScheduledAgentHosting {
    let result: ScheduledAgentExecutionObservation
    private(set) var executionCount = 0

    init(result: ScheduledAgentExecutionObservation) {
        self.result = result
    }

    func execute(_: ScheduledAgentExecutionRequest) async -> ScheduledAgentExecutionObservation {
        executionCount += 1
        return result
    }
}

@MainActor
private final class SuspendedScheduledAgentHost: CoreScheduledAgentHosting {
    private var continuation: CheckedContinuation<ScheduledAgentExecutionObservation, Never>?

    func execute(_: ScheduledAgentExecutionRequest) async -> ScheduledAgentExecutionObservation {
        await withCheckedContinuation { continuation = $0 }
    }

    func complete(with result: ScheduledAgentExecutionObservation) {
        continuation?.resume(returning: result)
        continuation = nil
    }
}

private actor ScheduledAgentUnusedFeedHost: CoreFeedHosting {
    func fetch(
        feedURL _: String,
        entityTag _: String?,
        lastModified _: String?,
        maximumResponseBytes _: UInt64,
        deadline _: Date?
    ) async -> HostObservation {
        .failed(code: .platformFailure, safeDetail: "Unexpected feed request")
    }
}

@MainActor
private final class ScheduledAgentUnusedPlaybackHost: CorePlaybackHosting {
    func execute(_: HostRequest) -> HostObservation {
        .failed(code: .platformFailure, safeDetail: "Unexpected playback request")
    }

    func installObservationSink(_: @escaping (PlaybackLifecycleObservation) -> Void) {}
}
