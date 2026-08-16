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

    func testNewerRegistrationFencesLateLoadAndDuplicateRefresh() async {
        let slowID = UUID()
        let fastID = UUID()
        let slow = publisherWorkflow(episodeID: slowID, stage: .requested, revision: 1)
        let fast = publisherWorkflow(episodeID: fastID, stage: .succeeded, revision: 2)
        let client = WorkflowClient(coalescingDelayNanoseconds: 0)
        client.attachPublisherChapterCore { query in
            if query.subjectIDs.contains(slowID) {
                try? await Task.sleep(nanoseconds: 150_000_000)
                return [slow]
            }
            return [fast]
        }

        let token = client.register(WorkflowProjectionRequest(
            subjectIDs: [slowID],
            kinds: [.publisherChapters]
        ))
        try? await Task.sleep(nanoseconds: 20_000_000)
        client.updateRegistration(token, request: WorkflowProjectionRequest(
            subjectIDs: [fastID],
            kinds: [.publisherChapters]
        ))
        await assertEventually {
            client.latest(kind: .publisherChapters, subjectID: fastID)?.state == .succeeded
        }
        try? await Task.sleep(nanoseconds: 180_000_000)
        XCTAssertNil(client.latest(kind: .publisherChapters, subjectID: slowID))

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

    func testChapterQueryRendersRustProjection() async throws {
        let episodeID = UUID()
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
            canCancel: true,
            retryAction: nil,
            cancelAction: nil
        )
        let client = WorkflowClient(coalescingDelayNanoseconds: 0)
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

    private func publisherWorkflow(
        episodeID: UUID,
        stage: PublisherChapterWorkflowStage,
        revision: UInt64
    ) -> PublisherChapterWorkflowProjection {
        PublisherChapterWorkflowProjection(
            episodeId: EpisodeId(uuid: episodeID),
            sourceVersion: "source-\(revision)",
            stage: stage,
            workflowRevision: StateRevision(value: revision),
            attempt: 1,
            maxAttempts: 5,
            requestId: HostRequestId(high: revision, low: 1),
            cancellationId: CancellationId(high: revision, low: 2),
            notBefore: nil,
            selectedArtifactId: nil,
            failure: nil,
            createdAt: UnixTimestampMilliseconds(value: Int64(revision)),
            updatedAt: UnixTimestampMilliseconds(value: Int64(revision)),
            canRetry: false,
            canCancel: stage != .succeeded,
            retryAction: nil,
            cancelAction: nil
        )
    }
}
