import Foundation
import XCTest
@testable import Podcastr

final class JobStoreExternalRecoveryTests: XCTestCase {
    private var fileURL: URL!
    private var store: JobStore!

    override func setUp() {
        super.setUp()
        fileURL = Persistence.episodeStoreURL(for: AppStateTestSupport.uniqueTempFileURL())
        store = JobStore(fileURL: fileURL)
    }

    override func tearDown() {
        if let fileURL {
            for suffix in ["", "-wal", "-shm"] {
                try? FileManager.default.removeItem(
                    at: URL(fileURLWithPath: fileURL.path + suffix)
                )
            }
        }
        store = nil
        fileURL = nil
        super.tearDown()
    }

    func testExternalOperationIdentitySurvivesReconstruction() throws {
        let now = Date()
        let job = try claimRemote(key: "transcribe:v1", now: now)
        let token = try XCTUnwrap(job.leaseToken)
        try store.recordExternalOperation(
            id: job.id,
            leaseToken: token,
            provider: "assemblyAI",
            externalID: "provider-123",
            state: "submitted"
        )

        let reconstructed = try XCTUnwrap(
            JobStore(fileURL: fileURL).job(idempotencyKey: "transcribe:v1")
        )
        XCTAssertEqual(reconstructed.externalProvider, "assemblyAI")
        XCTAssertEqual(reconstructed.externalOperationID, "provider-123")
        XCTAssertEqual(reconstructed.externalOperationState, "submitted")
    }

    func testInterruptedSubmissionIntentBlocksInsteadOfDuplicating() throws {
        let now = Date(timeIntervalSince1970: 4_000)
        let job = try claimRemote(key: "unsafe-remote", now: now, leaseDuration: 1)
        try store.recordExternalSubmissionIntent(
            id: job.id,
            leaseToken: try XCTUnwrap(job.leaseToken),
            provider: "openRouterWhisper",
            now: now
        )

        try store.reclaimExpiredLeases(now: now.addingTimeInterval(2))

        let recovered = try XCTUnwrap(store.job(idempotencyKey: "unsafe-remote"))
        XCTAssertEqual(recovered.state, .blocked)
        XCTAssertEqual(recovered.lastErrorClass, .unsafeToRetry)
        XCTAssertEqual(recovered.externalProvider, "openRouterWhisper")
        XCTAssertNil(recovered.externalOperationID)
        XCTAssertEqual(recovered.externalOperationState, "submitting")
        XCTAssertTrue(try store.claimDueJobs(
            resourceClass: .remoteSTT,
            capacity: 1,
            now: now.addingTimeInterval(3),
            owner: "duplicate",
            leaseDuration: 1
        ).isEmpty)
    }

    func testInterruptedPublisherFetchIsSafelyRequeued() throws {
        let now = Date(timeIntervalSince1970: 5_000)
        let job = try claimRemote(key: "publisher-fetch", now: now, leaseDuration: 1)
        try store.recordExternalOperation(
            id: job.id,
            leaseToken: try XCTUnwrap(job.leaseToken),
            provider: "publisherTranscript",
            externalID: "input-v1",
            state: "fetching",
            now: now
        )

        try store.reclaimExpiredLeases(now: now.addingTimeInterval(2))

        XCTAssertEqual(
            try store.job(idempotencyKey: "publisher-fetch")?.state,
            .retryScheduled
        )
        let reclaimed = try store.claimDueJobs(
            resourceClass: .remoteSTT,
            capacity: 1,
            now: now.addingTimeInterval(2),
            owner: "safe-retry",
            leaseDuration: 60
        )
        XCTAssertEqual(reclaimed.map(\.id), [job.id])
    }

    func testResumableProviderIdentityBypassesPublisherFallbackOnRecovery() {
        XCTAssertFalse(TranscriptIngestService.shouldAttemptPublisher(
            userInitiated: false,
            externalProvider: "assemblyAI",
            externalOperationID: "provider-123"
        ))
        XCTAssertFalse(TranscriptIngestService.shouldAttemptPublisher(
            userInitiated: false,
            externalProvider: "elevenLabsScribe",
            externalOperationID: "scribe-123"
        ))
        XCTAssertTrue(TranscriptIngestService.shouldAttemptPublisher(
            userInitiated: false,
            externalProvider: "publisherTranscript",
            externalOperationID: "input-v1"
        ))
    }

    private func claimRemote(
        key: String,
        now: Date,
        leaseDuration: TimeInterval = 60
    ) throws -> WorkJob {
        _ = try store.ensureJob(
            DesiredJob(
                idempotencyKey: key,
                kind: .transcriptIngest,
                subjectID: UUID(),
                inputVersion: "v1",
                resourceClass: .remoteSTT
            ),
            notBefore: now
        )
        let job = try XCTUnwrap(try store.claimDueJobs(
            resourceClass: .remoteSTT,
            capacity: 1,
            now: now,
            owner: "provider",
            leaseDuration: leaseDuration
        ).first)
        try store.markRunning(
            id: job.id,
            leaseToken: try XCTUnwrap(job.leaseToken),
            now: now
        )
        return job
    }
}
