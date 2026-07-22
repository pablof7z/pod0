import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class WorkflowProjectionTests: XCTestCase {
    private var fileURL: URL!
    private var store: JobStore!

    override func setUp() async throws {
        try await super.setUp()
        fileURL = Persistence.episodeStoreURL(for: AppStateTestSupport.uniqueTempFileURL())
        store = JobStore(fileURL: fileURL)
    }

    override func tearDown() async throws {
        if let fileURL {
            for suffix in ["", "-wal", "-shm"] {
                try? FileManager.default.removeItem(
                    at: URL(fileURLWithPath: fileURL.path + suffix)
                )
            }
        }
        store = nil
        fileURL = nil
        try await super.tearDown()
    }

    func testLateSubscriberReceivesEveryVisibleLifecycleAndRestartHydrates() async throws {
        let blocked = UUID()
        let failed = UUID()
        let succeeded = UUID()
        try insert(subject: blocked, key: "blocked")
        try insert(subject: failed, key: "failed")
        try insert(subject: succeeded, key: "succeeded")

        let client = WorkflowClient(coalescingDelayNanoseconds: 0)
        client.attach(jobStore: store)
        let token = client.register(WorkflowProjectionRequest(
            subjectIDs: [blocked, failed, succeeded],
            kinds: [.metadataIndex]
        ))
        await assertEventually {
            client.latest(kind: .metadataIndex, subjectID: blocked)?.state == .pending
        }

        let blockedAttempt = try claim(subject: blocked)
        try store.markRunning(id: blockedAttempt.id, leaseToken: XCTUnwrap(blockedAttempt.leaseToken))
        await assertEventually {
            client.latest(kind: .metadataIndex, subjectID: blocked)?.state == .running
        }
        try store.markBlocked(
            id: blockedAttempt.id,
            leaseToken: XCTUnwrap(blockedAttempt.leaseToken),
            reason: JobFailure(classification: .missingCredential, message: "Add a key")
        )
        await assertEventually {
            client.latest(kind: .metadataIndex, subjectID: blocked)?.state == .blocked
        }

        let failedAttempt = try claim(subject: failed)
        try store.markRunning(id: failedAttempt.id, leaseToken: XCTUnwrap(failedAttempt.leaseToken))
        try store.markFailedPermanent(
            id: failedAttempt.id,
            leaseToken: XCTUnwrap(failedAttempt.leaseToken),
            error: JobFailure(classification: .invalidInput, message: "Bad input")
        )
        await assertEventually {
            client.latest(kind: .metadataIndex, subjectID: failed)?.state == .failedPermanent
        }

        let succeededAttempt = try claim(subject: succeeded)
        try store.markRunning(
            id: succeededAttempt.id,
            leaseToken: XCTUnwrap(succeededAttempt.leaseToken)
        )
        try store.complete(
            id: succeededAttempt.id,
            leaseToken: XCTUnwrap(succeededAttempt.leaseToken),
            outputVersion: "v1"
        )
        await assertEventually {
            client.latest(kind: .metadataIndex, subjectID: succeeded)?.state == .succeeded
        }

        try store.manuallyRetry(kind: .metadataIndex, subjectID: blocked)
        let cancelledAttempt = try claim(subject: blocked)
        try store.markCancelled(
            id: cancelledAttempt.id,
            leaseToken: XCTUnwrap(cancelledAttempt.leaseToken)
        )
        await assertEventually {
            client.latest(kind: .metadataIndex, subjectID: blocked)?.state == .cancelled
        }
        client.unregister(token)
        await assertEventually {
            client.latest(kind: .metadataIndex, subjectID: blocked) == nil
        }

        let relaunched = WorkflowClient(coalescingDelayNanoseconds: 0)
        relaunched.attach(jobStore: JobStore(fileURL: fileURL))
        _ = relaunched.register(WorkflowProjectionRequest(
            subjectIDs: [blocked],
            kinds: [.metadataIndex]
        ))
        await assertEventually {
            relaunched.latest(kind: .metadataIndex, subjectID: blocked)?.state == .cancelled
        }
    }

    func testNewerRegistrationFencesLateLoadAndDuplicateRefresh() async {
        let slowID = UUID()
        let fastID = UUID()
        let slow = projection(subject: slowID, state: .running, updatedAt: Date(timeIntervalSince1970: 1))
        let fast = projection(subject: fastID, state: .succeeded, updatedAt: Date(timeIntervalSince1970: 2))
        let client = WorkflowClient(loader: { query in
            if query.subjectIDs.contains(slowID) {
                try? await Task.sleep(nanoseconds: 150_000_000)
                return [slow]
            }
            return [fast]
        }, coalescingDelayNanoseconds: 0)

        let token = client.register(WorkflowProjectionRequest(
            subjectIDs: [slowID],
            kinds: [.download]
        ))
        try? await Task.sleep(nanoseconds: 20_000_000)
        client.updateRegistration(token, request: WorkflowProjectionRequest(
            subjectIDs: [fastID],
            kinds: [.download]
        ))
        await assertEventually {
            client.latest(kind: .download, subjectID: fastID)?.state == .succeeded
        }
        try? await Task.sleep(nanoseconds: 180_000_000)
        XCTAssertNil(client.latest(kind: .download, subjectID: slowID))

        let revision = client.revision
        client.refresh(immediately: true)
        try? await Task.sleep(nanoseconds: 20_000_000)
        XCTAssertEqual(client.revision, revision)
    }

    func testAttentionQueryIsBoundedAndExcludesSuccessfulHistory() throws {
        let activeID = UUID()
        let succeededID = UUID()
        try insert(subject: succeededID, key: "done")
        try insert(subject: activeID, key: "active")
        let attempt = try claim(subject: succeededID)
        try store.markRunning(id: attempt.id, leaseToken: XCTUnwrap(attempt.leaseToken))
        try store.complete(
            id: attempt.id,
            leaseToken: XCTUnwrap(attempt.leaseToken),
            outputVersion: "audio"
        )

        let attention = try store.projections(for: WorkflowProjectionQuery(
            subjectIDs: [], kinds: [], attentionKinds: [.metadataIndex], recentKinds: [], limit: 1
        ))
        XCTAssertEqual(attention.count, 1)
        XCTAssertEqual(attention.first?.subjectID, activeID)
        let terminal = try store.projections(for: WorkflowProjectionQuery(
            subjectIDs: [succeededID], kinds: [.metadataIndex], attentionKinds: [],
            recentKinds: [], limit: 10
        ))
        XCTAssertEqual(terminal.first?.state, .succeeded)
    }

    func testRecentQueryKeepsDistinctRowsForSameSubjectAndHonorsLimit() throws {
        let subjectID = UUID()
        try insert(subject: subjectID, key: "history-1")
        try insert(subject: subjectID, key: "history-2")
        let history = try store.projections(for: WorkflowProjectionQuery(
            subjectIDs: [], kinds: [], attentionKinds: [],
            recentKinds: [.metadataIndex], limit: 10
        ))
        XCTAssertEqual(history.count, 2)
        let bounded = try store.projections(for: WorkflowProjectionQuery(
            subjectIDs: [], kinds: [], attentionKinds: [],
            recentKinds: [.metadataIndex], limit: 1
        ))
        XCTAssertEqual(bounded.count, 1)
    }

    func testChapterQueryIgnoresLegacyRowsAndRendersOnlyRustProjection() async throws {
        let episodeID = UUID()
        try LegacyChapterWorkflowTestSupport.insert(
            LegacyChapterWorkflowTestSupport.makeJob(
                key: "retired-publisher", kind: .publisherChapters,
                episodeID: episodeID, inputVersion: "legacy-source"
            ),
            into: store
        )
        let core = PublisherChapterWorkflowProjection(
            episodeId: EpisodeId(uuid: episodeID),
            sourceVersion: "rust-source",
            stage: .requested,
            workflowRevision: StateRevision(value: 3),
            attempt: 1,
            maxAttempts: 5,
            requestId: HostRequestId(high: 1, low: 2),
            cancellationId: CancellationId(high: 3, low: 4),
            notBefore: UnixTimestampMilliseconds(value: 1_000),
            selectedArtifactId: nil,
            failure: nil,
            createdAt: UnixTimestampMilliseconds(value: 900),
            updatedAt: UnixTimestampMilliseconds(value: 1_000),
            canRetry: false,
            canCancel: true
        )
        let client = WorkflowClient(coalescingDelayNanoseconds: 0)
        client.attach(jobStore: store)
        client.attachPublisherChapterCore { _ in [core] }
        _ = client.register(WorkflowProjectionRequest(
            subjectIDs: [episodeID],
            kinds: [.publisherChapters]
        ))

        await assertEventually {
            client.latest(kind: .publisherChapters, subjectID: episodeID)?.authority
                == .sharedRustPublisherChapters
        }
        XCTAssertEqual(
            client.latest(kind: .publisherChapters, subjectID: episodeID)?.coreWorkflowRevision,
            3
        )
        XCTAssertEqual(
            try store.legacyChapterJobs(kind: .publisherChapters).map(\.idempotencyKey),
            ["retired-publisher"]
        )
    }

    private func insert(
        subject: UUID,
        key: String,
        kind: WorkJobKind = .metadataIndex
    ) throws {
        _ = try store.ensureJob(DesiredJob(
            idempotencyKey: key,
            kind: kind,
            subjectID: subject,
            inputVersion: "v1",
            resourceClass: kind == .download ? .download : .embedding
        ), notBefore: .distantPast)
    }

    private func claim(
        subject: UUID,
        resourceClass: WorkResourceClass = .embedding
    ) throws -> WorkJob {
        try XCTUnwrap(try store.claimDueJobs(
            resourceClass: resourceClass,
            capacity: 1,
            now: Date(),
            owner: subject.uuidString,
            leaseDuration: 60
        ).first { $0.subjectID == subject })
    }

    private func eventually(
        timeoutNanoseconds: UInt64 = 1_000_000_000,
        condition: @MainActor () -> Bool
    ) async -> Bool {
        let started = DispatchTime.now().uptimeNanoseconds
        while DispatchTime.now().uptimeNanoseconds - started < timeoutNanoseconds {
            if condition() { return true }
            try? await Task.sleep(nanoseconds: 10_000_000)
        }
        return condition()
    }

    private func assertEventually(
        file: StaticString = #filePath,
        line: UInt = #line,
        condition: @MainActor () -> Bool
    ) async {
        let result = await eventually(condition: condition)
        XCTAssertTrue(result, file: file, line: line)
    }

    private func projection(
        subject: UUID,
        state: WorkJobState,
        updatedAt: Date
    ) -> WorkflowJobProjection {
        WorkflowJobProjection(job: WorkJob(
            id: UUID(), idempotencyKey: subject.uuidString, kind: .download,
            subjectID: subject, inputVersion: "v1", occurrenceID: nil,
            payloadVersion: 1, payload: nil, state: state, priority: 0,
            resourceClass: .download, attempt: 1, maxAttempts: 8,
            notBefore: updatedAt, leaseToken: nil, leaseOwner: nil,
            leaseExpiresAt: nil, externalProvider: nil, externalOperationID: nil,
            externalOperationState: nil, outputVersion: nil, lastErrorClass: nil,
            lastErrorMessage: nil, createdAt: updatedAt, updatedAt: updatedAt
        ))
    }
}
