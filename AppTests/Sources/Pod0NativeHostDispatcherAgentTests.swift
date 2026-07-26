import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class Pod0NativeHostDispatcherAgentTests: XCTestCase {
    func testDuplicateAgentRequestExecutesNativeEffectOnceAcrossRetirement() async throws {
        let outboxURL = temporaryOutboxURL()
        defer { try? FileManager.default.removeItem(at: outboxURL) }
        let outbox = try NativeHostObservationOutbox(fileURL: outboxURL)
        let agent = ControlledDispatcherAgentHost()
        let dispatcher = makeDispatcher(agent: agent, outbox: outbox)
        let request = agentEnvelope(requestID: 1)
        let facade = Pod0Facade()
        let started = expectation(description: "agent capability started")
        let retired = expectation(description: "agent observation retired")
        agent.onStart = { started.fulfill() }

        dispatcher.execute(request) { observation in
            dispatcher.record(observation, for: request, in: facade) {
                retired.fulfill()
            }
        }
        dispatcher.execute(request) { _ in
            XCTFail("Duplicate active request must not deliver")
        }

        await fulfillment(of: [started], timeout: 1)
        XCTAssertEqual(agent.callCount, 1)

        agent.complete(with: capabilityObservation(requestID: 1))
        await fulfillment(of: [retired], timeout: 1)

        dispatcher.execute(request) { _ in
            XCTFail("Retired request must not execute again")
        }
        await Task.yield()

        XCTAssertEqual(agent.callCount, 1)
        XCTAssertTrue(dispatcher.isKnown(request.requestId))
    }

    func testAgentCancellationReleasesTaskAndSuppressesLateResult() async throws {
        let outboxURL = temporaryOutboxURL()
        defer { try? FileManager.default.removeItem(at: outboxURL) }
        let outbox = try NativeHostObservationOutbox(fileURL: outboxURL)
        let agent = ControlledDispatcherAgentHost()
        let dispatcher = makeDispatcher(agent: agent, outbox: outbox)
        let request = agentEnvelope(requestID: 2)
        let started = expectation(description: "agent capability started")
        agent.onStart = { started.fulfill() }
        var observations: [HostObservationEnvelope] = []

        dispatcher.execute(request) { observations.append($0) }
        await fulfillment(of: [started], timeout: 1)

        dispatcher.cancel(
            requestID: request.requestId,
            cancellationID: request.cancellationId
        )
        await Task.yield()

        XCTAssertTrue(dispatcher.activeTasks.isEmpty)
        XCTAssertTrue(dispatcher.isKnown(request.requestId))
        XCTAssertTrue(agent.taskWasCancelled)

        agent.complete(with: capabilityObservation(requestID: 2))
        await Task.yield()
        dispatcher.execute(request) { observations.append($0) }
        await Task.yield()

        XCTAssertTrue(observations.isEmpty)
        XCTAssertEqual(agent.callCount, 1)
    }

    private func makeDispatcher(
        agent: ControlledDispatcherAgentHost,
        outbox: NativeHostObservationOutbox
    ) -> Pod0NativeHostDispatcher {
        Pod0NativeHostDispatcher(
            feedHost: DispatcherAgentFeedHost(),
            agentHost: agent,
            playbackHost: DispatcherAgentPlaybackHost(),
            observationOutbox: outbox
        )
    }

    private func temporaryOutboxURL() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("agent-dispatcher-\(UUID().uuidString).json")
    }

    private func agentEnvelope(requestID: UInt64) -> HostRequestEnvelope {
        HostRequestEnvelope(
            requestId: HostRequestId(high: 0, low: requestID),
            commandId: CommandId(high: 1, low: requestID),
            cancellationId: CancellationId(high: 2, low: requestID),
            issuedRevision: StateRevision(value: 3),
            deadlineAt: nil,
            request: .executeAgentCapability(capability: AgentCapabilityRequest(
                turnId: AgentTurnId(high: 4, low: requestID),
                proposalId: AgentProposalId(high: 5, low: requestID),
                proposalDigest: ContentDigest(
                    word0: 6,
                    word1: 7,
                    word2: 8,
                    word3: requestID
                ),
                executionFenceId: AgentExecutionFenceId(high: 9, low: requestID),
                executionMode: .perform,
                generatedAudioTarget: nil,
                action: .noArguments(tool: .pausePlayback)
            ))
        )
    }

    private func capabilityObservation(requestID: UInt64) -> HostObservation {
        .agentCapabilityObserved(
            turnId: AgentTurnId(high: 4, low: requestID),
            proposalId: AgentProposalId(high: 5, low: requestID),
            executionFenceId: AgentExecutionFenceId(high: 9, low: requestID),
            outcome: .succeeded(boundedResult: #"{"paused":true}"#)
        )
    }
}

@MainActor
private final class ControlledDispatcherAgentHost: CoreAgentHosting {
    private(set) var callCount = 0
    private(set) var taskWasCancelled = false
    var onStart: (() -> Void)?
    private var continuation: CheckedContinuation<HostObservation, Never>?

    func execute(_: HostRequest) async -> HostObservation {
        callCount += 1
        onStart?()
        return await withTaskCancellationHandler {
            await withCheckedContinuation { continuation = $0 }
        } onCancel: {
            Task { @MainActor [weak self] in
                self?.taskWasCancelled = true
            }
        }
    }

    func complete(with observation: HostObservation) {
        continuation?.resume(returning: observation)
        continuation = nil
    }
}

private actor DispatcherAgentFeedHost: CoreFeedHosting {
    func fetch(
        feedURL _: String,
        entityTag _: String?,
        lastModified _: String?,
        maximumResponseBytes _: UInt64,
        deadline _: Date?
    ) async -> HostObservation {
        .failed(code: .invalidResponse, safeDetail: "Unexpected feed request")
    }
}

@MainActor
private final class DispatcherAgentPlaybackHost: CorePlaybackHosting {
    func execute(_: HostRequest) -> HostObservation {
        .failed(code: .invalidResponse, safeDetail: "Unexpected playback request")
    }

    func installObservationSink(_: @escaping (PlaybackLifecycleObservation) -> Void) {}
}
