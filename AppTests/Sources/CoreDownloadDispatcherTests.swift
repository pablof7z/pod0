import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class CoreDownloadDispatcherTests: XCTestCase {
    func testDispatcherCorrelatesOrderedDownloadEventsAndExecutesRequestOnce() {
        let host = RecordingDownloadHost()
        let dispatcher = Pod0NativeHostDispatcher(
            feedHost: DownloadDispatcherFeedHost(),
            downloadHost: host,
            playbackHost: DownloadDispatcherPlaybackHost()
        )
        let request = envelope(requestLow: 1)
        var observations: [HostObservationEnvelope] = []

        dispatcher.execute(request) { observations.append($0) }
        dispatcher.execute(request) { _ in XCTFail("Duplicate request executed") }
        host.emit(
            requestID: request.requestId,
            sequence: 1,
            observation: acceptedObservation(request)
        )
        host.emit(
            requestID: request.requestId,
            sequence: 2,
            observation: stagedObservation(request)
        )

        XCTAssertEqual(host.executeCount, 1)
        XCTAssertEqual(observations.map(\.sequenceNumber), [1, 2])
        XCTAssertTrue(observations.allSatisfy { value in
            value.requestId == request.requestId
                && value.cancellationId == request.cancellationId
                && value.observedRequestRevision == request.issuedRevision
        })
    }

    func testExactCoreCancellationDetachesDownloadAndSuppressesLateCallback() {
        let host = RecordingDownloadHost()
        let dispatcher = Pod0NativeHostDispatcher(
            feedHost: DownloadDispatcherFeedHost(),
            downloadHost: host,
            playbackHost: DownloadDispatcherPlaybackHost()
        )
        let request = envelope(requestLow: 2)
        var observations: [HostObservationEnvelope] = []
        dispatcher.execute(request) { observations.append($0) }

        dispatcher.cancel(
            requestID: request.requestId,
            cancellationID: request.cancellationId
        )
        host.emit(
            requestID: request.requestId,
            sequence: 2,
            observation: stagedObservation(request)
        )

        XCTAssertEqual(host.cancelledRequestIDs, [request.requestId])
        XCTAssertTrue(observations.isEmpty)
        XCTAssertTrue(dispatcher.downloadRequests.isEmpty)
    }

    func testRelaunchOutboxReplayNotifiesNativeHostToRetireStagedEvidence() async throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(
            "pod0-download-replay-\(UUID().uuidString).json"
        )
        defer { try? FileManager.default.removeItem(at: url) }
        let outbox = try NativeHostObservationOutbox(fileURL: url)
        let request = envelope(requestLow: 3)
        let observation = HostObservationEnvelope(
            requestId: request.requestId,
            cancellationId: request.cancellationId,
            observedRequestRevision: request.issuedRevision,
            sequenceNumber: 2,
            observedAt: UnixTimestampMilliseconds(value: 1_800_000_000_000),
            observation: stagedObservation(request)
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
        XCTAssertEqual(host.retiredRequestIDs, [request.requestId])
        let pendingCount = await outbox.pendingCount()
        XCTAssertEqual(pendingCount, 0)
    }

    func testOrphanCallbackIsOfferedToRustAndRejectedWithoutMutation() async throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(
            "pod0-download-orphan-\(UUID().uuidString).json"
        )
        defer { try? FileManager.default.removeItem(at: url) }
        let outbox = try NativeHostObservationOutbox(fileURL: url)
        let host = RecordingDownloadHost()
        let dispatcher = Pod0NativeHostDispatcher(
            feedHost: DownloadDispatcherFeedHost(),
            downloadHost: host,
            playbackHost: DownloadDispatcherPlaybackHost(),
            observationOutbox: outbox
        )
        let facade = Pod0Facade()
        dispatcher.bindDownloadOrphanObservations(to: facade)
        let request = envelope(requestLow: 404)
        let identity = try XCTUnwrap(CoreDownloadTaskIdentity(request))

        host.emitOrphan(CoreDownloadOrphanObservation(
            identity: identity,
            sequenceNumber: 2,
            observation: stagedObservation(request)
        ))
        for _ in 0 ..< 100 where host.retiredReceipts.isEmpty {
            try await Task.sleep(for: .milliseconds(10))
        }

        guard case let .rejected(requestID, reason) = host.retiredReceipts.first else {
            return XCTFail("Expected Rust to reject the orphan observation")
        }
        XCTAssertEqual(requestID, request.requestId)
        XCTAssertEqual(reason, .unknownRequest)
        let pendingCount = await outbox.pendingCount()
        XCTAssertEqual(pendingCount, 0)
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
