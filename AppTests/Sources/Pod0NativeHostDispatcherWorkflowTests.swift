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

        dispatcher.executePendingRequests(from: facade)
        await Task.yield()

        XCTAssertEqual(dispatcher.activeTasks.count, 2)
        XCTAssertEqual(facade.nextHostRequests(maximumCount: 64).count, 1)
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
