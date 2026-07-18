import Foundation
import Pod0Core
import XCTest

final class Pod0CoreBindingTests: XCTestCase {
    func testGeneratedFacadeRoundTripsCommandsProjectionsAndSubscriptionLifecycle() throws {
        let facade = Pod0Facade()
        let subscriber = RecordingCoreSubscriber()
        let request = ProjectionRequest(scope: .library, maxItems: 20)
        let handle = facade.subscribe(request: request, subscriber: subscriber)

        XCTAssertEqual(subscriber.revisions, [0])

        facade.dispatch(
            command: CommandEnvelope(
                commandId: CommandId(high: 0, low: 1),
                cancellationId: CancellationId(high: 0, low: 2),
                expectedRevision: nil,
                command: .unsupported(wireCode: 77)
            )
        )

        XCTAssertEqual(subscriber.revisions, [0, 1])
        let projection = facade.snapshot(request: request)
        guard case let .library(value) = projection.projection else {
            return XCTFail("Expected a bounded library projection")
        }
        XCTAssertEqual(value.operations.count, 1)
        let unsupportedOperation = value.operations[0]
        XCTAssertEqual(unsupportedOperation.commandId, CommandId(high: 0, low: 1))
        XCTAssertEqual(unsupportedOperation.cancellationId, CancellationId(high: 0, low: 2))
        XCTAssertEqual(unsupportedOperation.stage, .failed)
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

        XCTAssertTrue(facade.nextHostRequests(maximumCount: 64).isEmpty)
        let cancelledProjection = facade.snapshot(request: request)
        guard case let .library(cancelledValue) = cancelledProjection.projection else {
            return XCTFail("Expected a library projection after cancellation")
        }
        XCTAssertTrue(cancelledValue.operations.contains { operation in
            operation.commandId == CommandId(high: 0, low: 3)
                && operation.stage == .cancelled
                && operation.failure?.code == .cancelled
        })

        facade.unsubscribe(subscriptionId: handle)
        facade.dispatch(
            command: CommandEnvelope(
                commandId: CommandId(high: 0, low: 7),
                cancellationId: CancellationId(high: 0, low: 8),
                expectedRevision: nil,
                command: .unsupported(wireCode: 78)
            )
        )
        XCTAssertEqual(subscriber.revisions, [0, 1, 2, 3])
    }
}

private final class RecordingCoreSubscriber: ProjectionSubscriber, @unchecked Sendable {
    private let lock = NSLock()
    private var storedRevisions: [UInt64] = []

    var revisions: [UInt64] {
        lock.withLock { storedRevisions }
    }

    func receive(projection: ProjectionEnvelope) throws {
        lock.withLock {
            storedRevisions.append(projection.stateRevision.value)
        }
    }
}
