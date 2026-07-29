import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

/// Round-trips the generated facade against a real durable store staged by
/// the production bootstrap, mirroring the Rust-side feed command lifecycle
/// fixtures. From contract version 53 a feed command succeeds once its
/// intent is durably committed; cancellation withdraws the pending fetch
/// without un-committing the subscription.
@MainActor
final class Pod0CoreFacadeLifecycleTests: XCTestCase {
    func testGeneratedFacadeRoundTripsCommandsProjectionsAndSubscriptionLifecycle() async throws {
        let made = AppStateTestSupport.makeIsolatedStore(
            sharedFeedHost: QueuedCoreFeedHost([])
        )
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let client = try XCTUnwrap(made.store.sharedLibrary)
        let facade = client.facade
        client.shutdown()
        let subscriber = RecordingCoreSubscriber()
        let request = ProjectionRequest(scope: .library, offset: 0, maxItems: 20)
        let handle = facade.subscribe(request: request, subscriber: subscriber)
        let baseline = try XCTUnwrap(subscriber.revisions.last)

        XCTAssertEqual(subscriber.revisions, [baseline])

        facade.dispatch(
            command: CommandEnvelope(
                commandId: CommandId(high: 0, low: 1),
                cancellationId: CancellationId(high: 0, low: 2),
                expectedRevision: nil,
                command: .unsupported(wireCode: 77)
            )
        )

        XCTAssertEqual(subscriber.revisions, [baseline, baseline + 1])
        let projection = facade.snapshot(request: request)
        XCTAssertEqual(projection.contractVersion, 53)
        guard case let .library(value) = projection.projection else {
            return XCTFail("Expected a bounded library projection")
        }
        let unsupportedOperation = try XCTUnwrap(value.operations.first {
            $0.commandId == CommandId(high: 0, low: 1)
        })
        XCTAssertEqual(unsupportedOperation.cancellationId, CancellationId(high: 0, low: 2))
        XCTAssertTrue(unsupportedOperation.stage == OperationStage.failed)
        XCTAssertEqual(unsupportedOperation.failure?.code, .unsupported(wireCode: 77))
        XCTAssertNil(unsupportedOperation.failure?.safeDetail)

        facade.dispatch(
            command: CommandEnvelope(
                commandId: CommandId(high: 0, low: 3),
                cancellationId: CancellationId(high: 0, low: 4),
                expectedRevision: nil,
                command: .subscribeToFeed(feedUrl: "https://example.test/feed")
            )
        )
        facade.dispatch(
            command: CommandEnvelope(
                commandId: CommandId(high: 0, low: 5),
                cancellationId: CancellationId(high: 0, low: 6),
                expectedRevision: nil,
                command: .cancelOperation(cancellationId: CancellationId(high: 0, low: 4))
            )
        )

        // Cancellation withdraws the queued fetch before native code claims it.
        XCTAssertTrue(facade.nextHostRequests(maximumCount: 64).allSatisfy { envelope in
            if case .fetchFeed = envelope.request { return false }
            return true
        })
        let cancelledProjection = facade.snapshot(request: request)
        guard case let .library(cancelledValue) = cancelledProjection.projection else {
            return XCTFail("Expected a library projection after cancellation")
        }
        // Contract 53: the subscribe operation succeeded at durable commit, so
        // cancelling afterwards stops the fetch workflow but leaves the
        // committed subscription and its terminal operation untouched.
        let subscribeOperation = try XCTUnwrap(cancelledValue.operations.first {
            $0.commandId == CommandId(high: 0, low: 3)
        })
        XCTAssertEqual(subscribeOperation.stage, OperationStage.succeeded)
        guard case .podcast(let committedPodcastID)? = subscribeOperation.result else {
            return XCTFail("Expected the subscribe operation to commit a podcast")
        }
        XCTAssertEqual(cancelledValue.subscriptions.map(\.podcastId), [committedPodcastID])
        XCTAssertTrue(
            cancelledValue.feedFetches.isEmpty,
            "Cancellation must retire the durable fetch workflow"
        )

        facade.unsubscribe(subscriptionId: handle)
        facade.dispatch(
            command: CommandEnvelope(
                commandId: CommandId(high: 0, low: 7),
                cancellationId: CancellationId(high: 0, low: 8),
                expectedRevision: nil,
                command: .unsupported(wireCode: 78)
            )
        )
        XCTAssertEqual(
            subscriber.revisions,
            [baseline, baseline + 1, baseline + 2, baseline + 3]
        )
    }
}

private final class RecordingCoreSubscriber: ProjectionSubscriber, @unchecked Sendable {
    private let lock = NSLock()
    private var storedRevisions: [UInt64] = []

    var revisions: [UInt64] {
        lock.withLock { storedRevisions }
    }

    func receive(projection: ProjectionEnvelope) {
        lock.withLock {
            storedRevisions.append(projection.stateRevision.value)
        }
    }
}
