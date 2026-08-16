import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

func leasedHostRequest(_ request: HostRequestEnvelope) -> LeasedHostRequestEnvelope {
    LeasedHostRequestEnvelope(
        lease: PersistedEffectLeaseIdentity(
            intentId: EffectIntentId(high: 101, low: 1),
            authorizingActivityId: ActivityId(high: 102, low: 1),
            correlationId: ActivityCorrelationId(high: 103, low: 1),
            attemptId: EffectAttemptId(high: 104, low: 1),
            leaseId: EffectLeaseId(high: 105, low: 1),
            fence: 1,
            expiresAt: UnixTimestampMilliseconds(value: 1_900_000_000_000)
        ),
        request: request
    )
}

@MainActor
final class CoreDownloadDispatcherTests: XCTestCase {
    func testDispatcherCorrelatesOrderedDownloadEventsAndExecutesRequestOnce() {
        let host = RecordingDownloadHost()
        let dispatcher = Pod0NativeHostDispatcher(
            feedHost: DownloadDispatcherFeedHost(),
            downloadHost: host,
            playbackHost: DownloadDispatcherPlaybackHost()
        )
        let request = leasedEnvelope(requestLow: 1)
        var observations: [LeasedHostObservationEnvelope] = []

        dispatcher.execute(request) { observations.append($0) }
        dispatcher.execute(request) { _ in XCTFail("Duplicate request executed") }
        host.emit(
            requestID: request.request.requestId,
            sequence: 1,
            observation: acceptedObservation(request.request)
        )
        host.emit(
            requestID: request.request.requestId,
            sequence: 2,
            observation: stagedObservation(request.request)
        )

        XCTAssertEqual(host.executeCount, 1)
        XCTAssertEqual(observations.map(\.observation.sequenceNumber), [1, 2])
        XCTAssertTrue(observations.allSatisfy { value in
            value.lease == request.lease
                && value.observation.requestId == request.request.requestId
                && value.observation.cancellationId == request.request.cancellationId
                && value.observation.observedRequestRevision == request.request.issuedRevision
        })
    }

    func testExactCoreCancellationDetachesDownloadAndSuppressesLateCallback() {
        let host = RecordingDownloadHost()
        let dispatcher = Pod0NativeHostDispatcher(
            feedHost: DownloadDispatcherFeedHost(),
            downloadHost: host,
            playbackHost: DownloadDispatcherPlaybackHost()
        )
        let request = leasedEnvelope(requestLow: 2)
        var observations: [LeasedHostObservationEnvelope] = []
        dispatcher.execute(request) { observations.append($0) }

        dispatcher.cancel(
            requestID: request.request.requestId,
            cancellationID: request.request.cancellationId
        )
        host.emit(
            requestID: request.request.requestId,
            sequence: 2,
            observation: stagedObservation(request.request)
        )

        XCTAssertEqual(host.cancelledRequestIDs, [request.request.requestId])
        XCTAssertTrue(observations.isEmpty)
        XCTAssertTrue(dispatcher.downloadRequests.isEmpty)
    }

    func testRelaunchOutboxReplayNotifiesNativeHostToRetireStagedEvidence() async throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(
            "pod0-download-replay-\(UUID().uuidString).json"
        )
        defer { try? FileManager.default.removeItem(at: url) }
        let outbox = try NativeHostObservationOutbox(fileURL: url)
        let request = leasedEnvelope(requestLow: 3)
        let observation = LeasedHostObservationEnvelope(
            lease: request.lease,
            observation: HostObservationEnvelope(
            requestId: request.request.requestId,
            cancellationId: request.request.cancellationId,
            observedRequestRevision: request.request.issuedRevision,
            sequenceNumber: 2,
            observedAt: UnixTimestampMilliseconds(value: 1_800_000_000_000),
            observation: stagedObservation(request.request)
            )
        )
        try await outbox.persistBeforeDelivery(observation)
        let host = RecordingDownloadHost()
        let dispatcher = Pod0NativeHostDispatcher(
            feedHost: DownloadDispatcherFeedHost(),
            downloadHost: host,
            playbackHost: DownloadDispatcherPlaybackHost(),
            observationOutbox: outbox
        )

        dispatcher.activateExecution()
        dispatcher.executePendingRequests(from: Pod0Facade())
        for _ in 0 ..< 100 where !dispatcher.observationRecoveryReady {
            try await Task.sleep(for: .milliseconds(10))
        }

        XCTAssertTrue(dispatcher.observationRecoveryReady)
        XCTAssertTrue(host.retiredRequestIDs.isEmpty)
        let pendingCount = await outbox.pendingCount()
        XCTAssertEqual(pendingCount, 1)
    }

    private func envelope(requestLow: UInt64) -> HostRequestEnvelope {
        HostRequestEnvelope(
            requestId: HostRequestId(high: 1, low: requestLow),
            commandId: CommandId(high: 2, low: requestLow),
            cancellationId: CancellationId(high: 3, low: requestLow),
            issuedRevision: StateRevision(value: 4),
            deadlineAt: nil,
            request: .startEpisodeDownload(
                episodeId: EpisodeId(high: 5, low: 6),
                intentId: DownloadIntentId(high: 7, low: 8),
                attemptId: DownloadAttemptId(high: 9, low: 10),
                inputVersion: String(repeating: "c", count: 64),
                enclosureUrl: "https://example.test/audio.mp3",
                resumeKey: nil
            )
        )
    }

    private func leasedEnvelope(requestLow: UInt64) -> LeasedHostRequestEnvelope {
        LeasedHostRequestEnvelope(
            lease: PersistedEffectLeaseIdentity(
                intentId: EffectIntentId(high: 11, low: requestLow),
                authorizingActivityId: ActivityId(high: 12, low: requestLow),
                correlationId: ActivityCorrelationId(high: 13, low: requestLow),
                attemptId: EffectAttemptId(high: 14, low: requestLow),
                leaseId: EffectLeaseId(high: 15, low: requestLow),
                fence: 1,
                expiresAt: UnixTimestampMilliseconds(value: 1_800_000_010_000)
            ),
            request: envelope(requestLow: requestLow)
        )
    }

    private func acceptedObservation(_ envelope: HostRequestEnvelope) -> HostObservation {
        guard case let .startEpisodeDownload(episodeID, intentID, attemptID, _, _, _) =
            envelope.request else { fatalError("Expected start request") }
        return .downloadAccepted(
            episodeId: episodeID,
            intentId: intentID,
            attemptId: attemptID,
            externalTaskKey: "task-1",
            resumeKey: "v1/resume"
        )
    }

    private func stagedObservation(_ envelope: HostRequestEnvelope) -> HostObservation {
        guard case let .startEpisodeDownload(episodeID, intentID, attemptID, _, _, _) =
            envelope.request else { fatalError("Expected start request") }
        return .downloadStaged(
            episodeId: episodeID,
            intentId: intentID,
            attemptId: attemptID,
            stagedFilePath: "/tmp/staged.media",
            byteCount: 20
        )
    }
}

@MainActor
private final class RecordingDownloadHost: CoreDownloadHosting {
    private var deliveries: [HostRequestId: Delivery] = [:]
    private(set) var executeCount = 0
    private(set) var cancelledRequestIDs: [HostRequestId] = []
    private(set) var retiredRequestIDs: [HostRequestId] = []
    private(set) var retiredReceipts: [HostObservationReceipt] = []
    private var orphanSink: OrphanDelivery?

    func installOrphanObservationSink(_ sink: @escaping OrphanDelivery) {
        orphanSink = sink
    }

    func execute(_ envelope: HostRequestEnvelope, delivery: @escaping Delivery) {
        executeCount += 1
        deliveries[envelope.requestId] = delivery
    }

    func cancel(requestID: HostRequestId, cancellationID _: CancellationId) {
        cancelledRequestIDs.append(requestID)
        deliveries[requestID] = nil
    }

    func retire(
        requestID: HostRequestId,
        observation _: HostObservation,
        receipt: HostObservationReceipt
    ) {
        retiredRequestIDs.append(requestID)
        retiredReceipts.append(receipt)
    }

    func shutdown() { deliveries.removeAll() }

    func emit(requestID: HostRequestId, sequence: UInt64, observation: HostObservation) {
        deliveries[requestID]?(sequence, observation)
    }

    func emitOrphan(_ observation: CoreDownloadOrphanObservation) {
        orphanSink?(observation)
    }
}

private struct DownloadDispatcherFeedHost: CoreFeedHosting {
    func fetch(
        feedURL _: String,
        entityTag _: String?,
        lastModified _: String?,
        maximumResponseBytes _: UInt64,
        deadline _: Date?
    ) async -> HostObservation {
        .failed(code: .platformFailure, safeDetail: nil)
    }
}

@MainActor
private final class DownloadDispatcherPlaybackHost: CorePlaybackHosting {
    func execute(_: HostRequest) -> HostObservation {
        .failed(code: .invalidResponse, safeDetail: nil)
    }

    func installObservationSink(_: @escaping (PlaybackLifecycleObservation) -> Void) {}
}
