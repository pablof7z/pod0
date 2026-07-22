import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class NativeHostObservationOutboxTests: XCTestCase {
    func testStandardLimitsCanStageMaximumTranscriptObservation() {
        XCTAssertGreaterThanOrEqual(
            NativeHostObservationOutbox.Limits.standard.maximumEnvelopeBytes,
            40 * 1_024 * 1_024
        )
        XCTAssertGreaterThanOrEqual(
            NativeHostObservationOutbox.Limits.standard.maximumArchiveBytes,
            128 * 1_024 * 1_024
        )
    }

    func testExactTypedEvidenceRestoresAfterProcessDeath() async throws {
        let fileURL = temporaryFileURL()
        defer { try? FileManager.default.removeItem(at: fileURL.deletingLastPathComponent()) }
        let expected = envelope(requestLow: 1, sequence: 2, observation: completion())
        let first = try NativeHostObservationOutbox(fileURL: fileURL)

        let inserted = try await first.persistBeforeDelivery(expected)
        XCTAssertTrue(inserted)
        XCTAssertTrue(FileManager.default.fileExists(atPath: fileURL.path))

        let relaunched = try NativeHostObservationOutbox(fileURL: fileURL)
        let restored = await relaunched.pendingObservations()
        XCTAssertEqual(restored, [expected])
        let delivered = try await relaunched.deliverPending { observation in
            .persisted(requestId: observation.requestId, terminal: true)
        }
        XCTAssertEqual(delivered, 1)
    }

    func testNonterminalReceiptsRetainAndTerminalReceiptDeletes() async throws {
        let fileURL = temporaryFileURL()
        defer { try? FileManager.default.removeItem(at: fileURL.deletingLastPathComponent()) }
        let outbox = try NativeHostObservationOutbox(fileURL: fileURL)
        let accepted = envelope(
            requestLow: 2,
            sequence: 1,
            observation: .chapterModelProviderAccepted(
                episodeId: EpisodeId(high: 3, low: 4),
                generation: 5,
                submissionFenceId: ChapterModelSubmissionFenceId(high: 6, low: 7),
                update: ChapterModelProviderUpdate(
                    providerOperationId: "operation-1",
                    providerStatus: "running"
                )
            )
        )
        let completed = envelope(requestLow: 2, sequence: 2, observation: completion())
        try await outbox.persistBeforeDelivery(accepted)
        try await outbox.persistBeforeDelivery(completed)
        let requestID = accepted.requestId

        let retainedForRetry = try await outbox.acknowledge(.retainAndRetry(requestId: requestID))
        let persistedNonterminal = try await outbox.acknowledge(
            .persisted(requestId: requestID, terminal: false)
        )
        let acceptedTransient = try await outbox.acknowledge(
            .acceptedTransient(requestId: requestID)
        )
        XCTAssertFalse(retainedForRetry)
        XCTAssertFalse(persistedNonterminal)
        XCTAssertFalse(acceptedTransient)
        let retainedCount = await outbox.pendingCount()
        XCTAssertEqual(retainedCount, 2)

        let retired = try await outbox.acknowledge(
            .persisted(requestId: requestID, terminal: true)
        )
        XCTAssertTrue(retired)
        let pendingCount = await outbox.pendingCount()
        XCTAssertEqual(pendingCount, 0)
        let relaunched = try NativeHostObservationOutbox(fileURL: fileURL)
        let relaunchedCount = await relaunched.pendingCount()
        XCTAssertEqual(relaunchedCount, 0)
    }

    func testDeliverySeesDurableRecordBeforeTerminalAcknowledgement() async throws {
        let fileURL = temporaryFileURL()
        defer { try? FileManager.default.removeItem(at: fileURL.deletingLastPathComponent()) }
        let expected = envelope(requestLow: 3, sequence: 0, observation: completion())
        let outbox = try NativeHostObservationOutbox(fileURL: fileURL)
        let probe = ObservationProbe()

        let receipt = try await outbox.persistAndDeliver(expected) { observation in
            let disk = try! NativeHostObservationOutbox(fileURL: fileURL)
            await probe.capture(await disk.pendingObservations())
            return .persisted(requestId: observation.requestId, terminal: true)
        }

        XCTAssertEqual(receipt, .persisted(requestId: expected.requestId, terminal: true))
        let observed = await probe.observations()
        let pendingCount = await outbox.pendingCount()
        XCTAssertEqual(observed, [expected])
        XCTAssertEqual(pendingCount, 0)
    }

    func testBoundsRejectWithoutEvictingExistingEvidence() async throws {
        let fileURL = temporaryFileURL()
        defer { try? FileManager.default.removeItem(at: fileURL.deletingLastPathComponent()) }
        let limits = NativeHostObservationOutbox.Limits(
            maximumRecordCount: 1,
            maximumEnvelopeBytes: 1_024,
            maximumArchiveBytes: 4_096
        )
        let outbox = try NativeHostObservationOutbox(fileURL: fileURL, limits: limits)
        let first = envelope(requestLow: 4, sequence: 0, observation: .cancelled)
        try await outbox.persistBeforeDelivery(first)

        do {
            try await outbox.persistBeforeDelivery(
                envelope(requestLow: 4, sequence: 0, observation: .failed(
                    code: .platformFailure,
                    safeDetail: nil
                ))
            )
            XCTFail("Expected conflicting identity failure")
        } catch {
            XCTAssertEqual(
                error as? NativeHostObservationOutbox.OutboxError,
                .conflictingObservationIdentity
            )
        }

        do {
            try await outbox.persistBeforeDelivery(
                envelope(requestLow: 5, sequence: 0, observation: .cancelled)
            )
            XCTFail("Expected bounded capacity failure")
        } catch {
            XCTAssertEqual(
                error as? NativeHostObservationOutbox.OutboxError,
                .recordLimitExceeded
            )
        }
        let pending = await outbox.pendingObservations()
        XCTAssertEqual(pending, [first])
    }

    func testRestoreRejectsRequestMetadataTampering() async throws {
        let fileURL = temporaryFileURL()
        defer { try? FileManager.default.removeItem(at: fileURL.deletingLastPathComponent()) }
        let outbox = try NativeHostObservationOutbox(fileURL: fileURL)
        try await outbox.persistBeforeDelivery(
            envelope(requestLow: 6, sequence: 0, observation: .cancelled)
        )
        var json = try XCTUnwrap(
            JSONSerialization.jsonObject(with: Data(contentsOf: fileURL)) as? [String: Any]
        )
        var records = try XCTUnwrap(json["records"] as? [[String: Any]])
        records[0]["requestLow"] = 99
        json["records"] = records
        try JSONSerialization.data(withJSONObject: json).write(to: fileURL, options: .atomic)

        XCTAssertThrowsError(try NativeHostObservationOutbox(fileURL: fileURL)) { error in
            XCTAssertEqual(
                error as? NativeHostObservationOutbox.OutboxError,
                .invalidArchive
            )
        }
    }

    func testConcurrentDuplicatePersistenceIsIdempotent() async throws {
        let fileURL = temporaryFileURL()
        defer { try? FileManager.default.removeItem(at: fileURL.deletingLastPathComponent()) }
        let outbox = try NativeHostObservationOutbox(fileURL: fileURL)
        let expected = envelope(requestLow: 7, sequence: 0, observation: completion())

        try await withThrowingTaskGroup(of: Bool.self) { group in
            for _ in 0 ..< 8 {
                group.addTask { try await outbox.persistBeforeDelivery(expected) }
            }
            var inserted = 0
            for try await value in group where value { inserted += 1 }
            XCTAssertEqual(inserted, 1)
        }
        let pending = await outbox.pendingObservations()
        XCTAssertEqual(pending, [expected])
    }

    private func envelope(
        requestLow: UInt64,
        sequence: UInt64,
        observation: HostObservation
    ) -> HostObservationEnvelope {
        HostObservationEnvelope(
            requestId: HostRequestId(high: 1, low: requestLow),
            cancellationId: CancellationId(high: 2, low: requestLow),
            observedRequestRevision: StateRevision(value: 3),
            sequenceNumber: sequence,
            observedAt: UnixTimestampMilliseconds(value: 4),
            observation: observation
        )
    }

    private func completion() -> HostObservation {
        .chapterModelCompleted(
            episodeId: EpisodeId(high: 3, low: 4),
            generation: 5,
            submissionFenceId: ChapterModelSubmissionFenceId(high: 6, low: 7),
            completion: ChapterModelCompletionObservation(
                completion: #"{"chapters":[]}"#,
                provider: "openrouter",
                model: "model-a",
                promptTokens: 10,
                completionTokens: 4,
                cachedTokens: 0,
                reasoningTokens: 0,
                costMicrousd: 2,
                providerOperationId: "operation-1",
                providerStatus: "completed",
                providerGeneratedAt: nil
            )
        )
    }

    private func temporaryFileURL() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("native-outbox-\(UUID().uuidString)", isDirectory: true)
            .appendingPathComponent("outbox.json")
    }
}

private actor ObservationProbe {
    private var value: [HostObservationEnvelope] = []

    func capture(_ observations: [HostObservationEnvelope]) {
        value = observations
    }

    func observations() -> [HostObservationEnvelope] {
        value
    }
}
