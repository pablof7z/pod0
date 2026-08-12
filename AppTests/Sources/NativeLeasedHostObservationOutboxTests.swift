import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class NativeLeasedHostObservationOutboxTests: XCTestCase {
    func testExactLeaseAndObservationRestoreBeforeRustAcknowledgement() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("leased-outbox-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let fileURL = directory.appendingPathComponent("outbox.json")
        let expected = leasedEnvelope()
        let first = try NativeHostObservationOutbox(fileURL: fileURL)

        let persisted = try await first.persistBeforeDelivery(expected)
        XCTAssertTrue(persisted)
        let relaunched = try NativeHostObservationOutbox(fileURL: fileURL)
        let restored = await relaunched.pendingLeasedObservations()
        XCTAssertEqual(restored, [expected])
        for _ in 0 ..< 5 {
            let beganDelivery = await relaunched.beginDelivery(of: expected)
            XCTAssertTrue(beganDelivery)
        }
        let pending = await relaunched.pendingLeasedObservations()
        XCTAssertEqual(pending, [expected])
        await relaunched.finishDelivery(of: expected)
        let acknowledged = try await relaunched.acknowledgeLeased(
            .persisted(requestId: expected.observation.requestId, terminal: true)
        )
        XCTAssertTrue(acknowledged)
        let pendingCount = await relaunched.pendingCount()
        XCTAssertEqual(pendingCount, 0)
    }

    private func leasedEnvelope() -> LeasedHostObservationEnvelope {
        LeasedHostObservationEnvelope(
            lease: PersistedEffectLeaseIdentity(
                intentId: EffectIntentId(high: 1, low: 2),
                authorizingActivityId: ActivityId(high: 3, low: 4),
                correlationId: ActivityCorrelationId(high: 5, low: 6),
                attemptId: EffectAttemptId(high: 7, low: 8),
                leaseId: EffectLeaseId(high: 9, low: 10),
                fence: 11,
                expiresAt: UnixTimestampMilliseconds(value: 20)
            ),
            observation: HostObservationEnvelope(
                requestId: HostRequestId(high: 12, low: 13),
                cancellationId: CancellationId(high: 14, low: 15),
                observedRequestRevision: StateRevision(value: 16),
                sequenceNumber: 0,
                observedAt: UnixTimestampMilliseconds(value: 17),
                observation: .cancelled
            )
        )
    }
}
