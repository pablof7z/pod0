import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class Pod0NativeHostDispatcherRecallTests: XCTestCase {
    func testRecallCancellationEmitsOnceAndRejectsLateResult() async {
        let recall = ControlledRecallHost()
        let dispatcher = makeDispatcher(recall: recall)
        let request = envelope(id: 1)
        var observations: [HostObservationEnvelope] = []

        dispatcher.execute(request) { observations.append($0) }
        await recall.waitUntilStarted()
        dispatcher.cancel(cancellationID: request.cancellationId)
        await recall.finish(.recallQueryEmbedded(
            queryId: RecallQueryId(high: 2, low: 3),
            embedding: RecallEmbeddingVector(values: [1])
        ))
        await Task.yield()

        XCTAssertEqual(observations.map(\.observation), [.cancelled])
    }

    func testDispatcherTeardownCancelsWorkAndSuppressesLateDelivery() async {
        let recall = ControlledRecallHost()
        let dispatcher = makeDispatcher(recall: recall)
        var observations: [HostObservationEnvelope] = []

        dispatcher.execute(envelope(id: 2)) { observations.append($0) }
        await recall.waitUntilStarted()
        dispatcher.shutdown()
        await recall.finish(.failed(code: .platformFailure, safeDetail: nil))
        await Task.yield()

        XCTAssertTrue(observations.isEmpty)
    }

    private func makeDispatcher(recall: any CoreRecallHosting) -> Pod0NativeHostDispatcher {
        Pod0NativeHostDispatcher(
            feedHost: NoopRecallFeedHost(),
            playbackHost: NoopRecallPlaybackHost(),
            recallHost: recall
        )
    }

    private func envelope(id: UInt64) -> HostRequestEnvelope {
        HostRequestEnvelope(
            requestId: HostRequestId(high: 1, low: id),
            commandId: CommandId(high: 1, low: id),
            cancellationId: CancellationId(high: 1, low: id),
            issuedRevision: StateRevision(value: 1),
            deadlineAt: nil,
            request: .embedRecallQuery(
                queryId: RecallQueryId(high: 2, low: 3),
                provider: .openRouter,
                model: "embedding-model",
                text: "private query",
                maximumDimensions: 3
            )
        )
    }
}

private actor ControlledRecallHost: CoreRecallHosting {
    private var continuation: CheckedContinuation<HostObservation, Never>?
    private var started = false

    func execute(_ request: HostRequest) async -> HostObservation {
        started = true
        return await withCheckedContinuation { continuation = $0 }
    }

    func waitUntilStarted() async {
        while !started { await Task.yield() }
    }

    func finish(_ observation: HostObservation) {
        continuation?.resume(returning: observation)
        continuation = nil
    }
}

private struct NoopRecallFeedHost: CoreFeedHosting {
    func fetch(
        feedURL: String,
        entityTag: String?,
        lastModified: String?,
        maximumResponseBytes: UInt64,
        deadline: Date?
    ) async -> HostObservation {
        .failed(code: .platformFailure, safeDetail: nil)
    }
}

@MainActor
private final class NoopRecallPlaybackHost: CorePlaybackHosting {
    func execute(_ request: HostRequest) -> HostObservation {
        .failed(code: .platformFailure, safeDetail: nil)
    }

    func installObservationSink(_ sink: @escaping (PlaybackLifecycleObservation) -> Void) {}
}
