import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class Pod0NativeHostDispatcherWorkflowTests: XCTestCase {
    func testExactCoreCancellationIsSilentAndSuppressesLatePublisherCompletion() async {
        let dispatcher = Pod0NativeHostDispatcher(
            feedHost: WorkflowSuspendingFeedHost(),
            publisherChapterHost: WorkflowSuspendingPublisherHost(),
            playbackHost: WorkflowPlaybackHost()
        )
        let request = envelope(
            requestID: 20,
            request: .fetchPublisherChapters(
                episodeId: EpisodeId(high: 1, low: 2),
                sourceUrl: "https://example.test/chapters.json",
                notBefore: nil,
                maximumResponseBytes: 4_096
            )
        )
        var observations: [HostObservationEnvelope] = []

        dispatcher.execute(request) { observations.append($0) }
        await Task.yield()
        dispatcher.cancel(
            requestID: request.requestId,
            cancellationID: request.cancellationId
        )
        await Task.yield()
        await Task.yield()

        XCTAssertTrue(observations.isEmpty)
        XCTAssertTrue(dispatcher.activeTasks.isEmpty)
    }

    func testFacadeDrainNeverExceedsConfiguredNativeTaskCapacity() async {
        let dispatcher = Pod0NativeHostDispatcher(
            feedHost: WorkflowSuspendingFeedHost(),
            playbackHost: WorkflowPlaybackHost(),
            maximumConcurrentTasks: 2
        )
        let facade = Pod0Facade()
        for index in 0 ..< 3 {
            facade.dispatch(command: CommandEnvelope(
                commandId: CommandId(high: 30, low: UInt64(index)),
                cancellationId: CancellationId(high: 31, low: UInt64(index)),
                expectedRevision: nil,
                command: .subscribeToFeed(
                    feedUrl: "https://example.test/feed-\(index).xml"
                )
            ))
        }

        dispatcher.activateExecution()
        dispatcher.executePendingRequests(from: facade)
        await Task.yield()

        XCTAssertEqual(dispatcher.activeTasks.count, 2)
        XCTAssertEqual(facade.nextHostRequests(maximumCount: 64).count, 1)
        dispatcher.shutdown()
    }

    func testDurableObservationStagingFailureRetainsRequestWithoutNativePolling() async throws {
        let outboxURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("dispatcher-retained-\(UUID().uuidString).json")
        defer { try? FileManager.default.removeItem(at: outboxURL) }
        let outbox = try NativeHostObservationOutbox(
            fileURL: outboxURL,
            limits: .init(
                maximumRecordCount: 1,
                maximumEnvelopeBytes: 4_096,
                maximumArchiveBytes: 8_192
            )
        )
        try await outbox.persistBeforeDelivery(observation(requestID: 40))
        let dispatcher = Pod0NativeHostDispatcher(
            feedHost: WorkflowSuspendingFeedHost(),
            playbackHost: WorkflowPlaybackHost(),
            observationOutbox: outbox
        )
        dispatcher.observationRecoveryReady = true
        dispatcher.activateExecution()
        let request = envelope(
            requestID: 41,
            request: transcriptRecoveryRequest(requestID: 41)
        )
        var completionCount = 0

        dispatcher.record(
            observation(requestID: 41),
            for: request,
            in: Pod0Facade()
        ) { completionCount += 1 }
        let acknowledgement = try XCTUnwrap(dispatcher.acknowledgementTasks[request.requestId])
        await acknowledgement.task.value

        XCTAssertEqual(dispatcher.acknowledgementTasks.count, 1)
        XCTAssertEqual(dispatcher.retainedObservationIDs, [request.requestId])
        XCTAssertEqual(completionCount, 0)

        dispatcher.executePendingRequests(from: Pod0Facade())
        let retry = try XCTUnwrap(dispatcher.retainedObservationRetryTask)
        await retry.value

        XCTAssertEqual(dispatcher.acknowledgementTasks.count, 1)
        XCTAssertEqual(dispatcher.retainedObservationIDs, [request.requestId])
        XCTAssertEqual(completionCount, 0)
        dispatcher.shutdown()
    }

    private func envelope(requestID: UInt64, request: HostRequest) -> HostRequestEnvelope {
        HostRequestEnvelope(
            requestId: HostRequestId(high: 0, low: requestID),
            commandId: CommandId(high: 0, low: requestID),
            cancellationId: CancellationId(high: 9, low: requestID),
            issuedRevision: StateRevision(value: 7),
            deadlineAt: nil,
            request: request
        )
    }

    private func transcriptRecoveryRequest(requestID: UInt64) -> HostRequest {
        .executeTranscriptCapability(capability: .recoverProvider(
            context: TranscriptCapabilityContext(
                episodeId: EpisodeId(high: 1, low: requestID),
                podcastId: PodcastId(high: 2, low: requestID),
                sourceRevision: "audio-v1"
            ),
            attemptId: TranscriptAttemptId(high: 3, low: requestID),
            submissionFenceId: TranscriptSubmissionFenceId(high: 4, low: requestID),
            provider: .assemblyAi,
            model: "universal-3-pro",
            externalOperationId: "operation-\(requestID)",
            providerStatus: "processing",
            maximumResponseBytes: 1_000_000
        ))
    }

    private func observation(requestID: UInt64) -> HostObservationEnvelope {
        HostObservationEnvelope(
            requestId: HostRequestId(high: 0, low: requestID),
            cancellationId: CancellationId(high: 9, low: requestID),
            observedRequestRevision: StateRevision(value: 7),
            sequenceNumber: 0,
            observedAt: UnixTimestampMilliseconds(value: 1_700_000_000_000),
            observation: .failed(code: .platformFailure, safeDetail: "Bounded failure")
        )
    }
}

private actor WorkflowSuspendingFeedHost: CoreFeedHosting {
    func fetch(
        feedURL _: String,
        entityTag _: String?,
        lastModified _: String?,
        maximumResponseBytes _: UInt64,
        deadline _: Date?
    ) async -> HostObservation {
        do {
            try await Task.sleep(for: .seconds(30))
            return .failed(code: .platformFailure, safeDetail: "Unexpected completion")
        } catch {
            return .cancelled
        }
    }
}

private actor WorkflowSuspendingPublisherHost: CorePublisherChapterHosting {
    func fetch(
        episodeID _: EpisodeId,
        sourceURL _: String,
        maximumResponseBytes _: UInt64,
        deadline _: Date?
    ) async -> HostObservation {
        do {
            try await Task.sleep(for: .seconds(30))
            return .failed(code: .platformFailure, safeDetail: "Unexpected completion")
        } catch {
            return .cancelled
        }
    }
}

@MainActor
private final class WorkflowPlaybackHost: CorePlaybackHosting {
    func execute(_: HostRequest) -> HostObservation {
        .failed(code: .invalidResponse, safeDetail: "Unexpected playback request")
    }

    func installObservationSink(_: @escaping (PlaybackLifecycleObservation) -> Void) {}
}
