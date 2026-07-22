import Foundation
import XCTest
@testable import Podcastr

final class WorkflowJobActionTests: XCTestCase {
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

    func testActionAvailabilityIsExhaustiveAcrossLifecycleStates() {
        let expected: [WorkJobState: Set<WorkflowJobAction>] = [
            .pending: [.cancel],
            .leased: [.cancel],
            .running: [.cancel],
            .retryScheduled: [.cancel],
            .blocked: [.retry, .cancel],
            .failedPermanent: [.retry],
            .cancelled: [.retry],
            .obsolete: [],
            .succeeded: [],
        ]
        for state in WorkJobState.allCases {
            XCTAssertEqual(projection(state: state).allowedActions, expected[state])
        }
        XCTAssertEqual(
            projection(state: .blocked, errorClass: .unsafeToRetry).allowedActions,
            [.cancel]
        )
        XCTAssertEqual(
            projection(state: .blocked, errorClass: .unsupportedFormat).allowedActions,
            [.cancel]
        )
        XCTAssertTrue(
            projection(state: .failedPermanent, errorClass: .unsafeToRetry).allowedActions.isEmpty
        )
        XCTAssertTrue(
            projection(state: .failedPermanent, errorClass: .unsupportedFormat).allowedActions.isEmpty
        )
        XCTAssertTrue(
            projection(state: .failedPermanent, errorClass: .invalidInput).allowedActions.isEmpty
        )
    }

    func testActionsAreAtomicRevisionCheckedAndTerminalSafe() throws {
        let subjectID = UUID()
        _ = try store.ensureJob(desired(subjectID: subjectID), notBefore: .distantPast)
        let pending = WorkflowJobProjection(job: try XCTUnwrap(store.job(idempotencyKey: "action")))
        XCTAssertEqual(
            try store.perform(.cancel, jobID: pending.id, expectedUpdatedAt: pending.updatedAt),
            .accepted(.cancel)
        )
        XCTAssertEqual(
            try store.perform(.cancel, jobID: pending.id, expectedUpdatedAt: pending.updatedAt),
            .stale
        )

        let cancelled = WorkflowJobProjection(job: try XCTUnwrap(store.job(id: pending.id)))
        XCTAssertEqual(
            try store.perform(.retry, jobID: cancelled.id, expectedUpdatedAt: cancelled.updatedAt),
            .accepted(.retry)
        )
        let attempt = try XCTUnwrap(try store.claimDueJobs(
            resourceClass: .remoteSTT,
            capacity: 1,
            now: Date(),
            owner: "actions",
            leaseDuration: 60
        ).first)
        let token = try XCTUnwrap(attempt.leaseToken)
        try store.markRunning(id: attempt.id, leaseToken: token)
        let running = WorkflowJobProjection(job: try XCTUnwrap(store.job(id: attempt.id)))
        try store.complete(id: attempt.id, leaseToken: token, outputVersion: "v1")
        XCTAssertEqual(
            try store.perform(.retry, jobID: running.id, expectedUpdatedAt: running.updatedAt),
            .stale
        )
        let succeeded = WorkflowJobProjection(job: try XCTUnwrap(store.job(id: attempt.id)))
        XCTAssertEqual(
            try store.perform(.retry, jobID: succeeded.id, expectedUpdatedAt: succeeded.updatedAt),
            .alreadyComplete
        )
    }

    func testUnsafeInterruptedSubmissionCannotBeRetried() throws {
        _ = try store.ensureJob(desired(subjectID: UUID()), notBefore: .distantPast)
        let attempt = try XCTUnwrap(try store.claimDueJobs(
            resourceClass: .remoteSTT,
            capacity: 1,
            now: Date(),
            owner: "unsafe",
            leaseDuration: 60
        ).first)
        let token = try XCTUnwrap(attempt.leaseToken)
        try store.markRunning(id: attempt.id, leaseToken: token)
        try store.markBlocked(
            id: attempt.id,
            leaseToken: token,
            reason: JobFailure(
                classification: .unsafeToRetry,
                message: "provider submission may have completed"
            )
        )
        let blocked = WorkflowJobProjection(job: try XCTUnwrap(store.job(id: attempt.id)))
        XCTAssertEqual(
            try store.perform(.retry, jobID: blocked.id, expectedUpdatedAt: blocked.updatedAt),
            .notAllowed
        )
        XCTAssertEqual(try store.job(id: blocked.id)?.state, .blocked)
    }

    private func desired(subjectID: UUID) -> DesiredJob {
        DesiredJob(
            idempotencyKey: "action",
            kind: .transcriptIngest,
            subjectID: subjectID,
            inputVersion: "v1",
            resourceClass: .remoteSTT
        )
    }

    private func projection(
        state: WorkJobState,
        errorClass: JobErrorClass? = nil
    ) -> WorkflowJobProjection {
        let now = Date()
        return WorkflowJobProjection(job: WorkJob(
            id: UUID(), idempotencyKey: UUID().uuidString, kind: .transcriptIngest,
            subjectID: UUID(), inputVersion: "v1", occurrenceID: nil,
            payloadVersion: 1, payload: nil, state: state, priority: 0,
            resourceClass: .remoteSTT, attempt: 1, maxAttempts: 8,
            notBefore: now, leaseToken: nil, leaseOwner: nil,
            leaseExpiresAt: nil, externalProvider: nil, externalOperationID: nil,
            externalOperationState: nil, outputVersion: nil,
            lastErrorClass: errorClass, lastErrorMessage: nil,
            createdAt: now, updatedAt: now
        ))
    }
}
