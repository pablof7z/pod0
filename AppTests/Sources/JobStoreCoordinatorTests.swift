import Foundation
import XCTest
@testable import Podcastr

final class JobStoreCoordinatorTests: XCTestCase {
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

    func testCanonicalEnsureIsIdempotentAndTerminalRowsNeverReactivate() throws {
        let desired = makeDesired(key: "metadata:episode:v1")
        XCTAssertTrue(try store.ensureJob(desired))
        XCTAssertFalse(try store.ensureJob(desired))

        let claimed = try XCTUnwrap(try store.claimDueJobs(
            resourceClass: .embedding, capacity: 1, now: Date(),
            owner: "test", leaseDuration: 60
        ).first)
        try store.markRunning(id: claimed.id, leaseToken: try XCTUnwrap(claimed.leaseToken))
        try store.complete(
            id: claimed.id, leaseToken: try XCTUnwrap(claimed.leaseToken), outputVersion: "v1"
        )

        XCTAssertFalse(try store.ensureJob(desired))
        XCTAssertEqual(try store.allJobs().map(\.state), [.succeeded])
        XCTAssertTrue(try store.ensureJob(makeDesired(key: "metadata:episode:v2", version: "v2")))
        XCTAssertEqual(try store.allJobs().count, 2)
    }

    func testAtomicClaimUsesPriorityThenStableCreationOrder() throws {
        let now = Date()
        _ = try store.ensureJob(makeDesired(key: "low", priority: 10), notBefore: now)
        _ = try store.ensureJob(makeDesired(key: "high-a", priority: 100), notBefore: now)
        _ = try store.ensureJob(makeDesired(key: "high-b", priority: 100), notBefore: now)

        let claimed = try store.claimDueJobs(
            resourceClass: .embedding, capacity: 3, now: now,
            owner: "priority", leaseDuration: 60
        )
        XCTAssertEqual(claimed.map(\.idempotencyKey), ["high-a", "high-b", "low"])
        XCTAssertEqual(claimed.map(\.attempt), [1, 1, 1])
        XCTAssertEqual(Set(claimed.compactMap(\.leaseToken)).count, 3)
    }

    func testCapacityCountsLiveLeasesAcrossCoordinatorOwners() throws {
        let now = Date(timeIntervalSince1970: 2_000)
        for index in 0..<5 {
            _ = try store.ensureJob(
                makeDesired(key: "global-capacity-\(index)"),
                notBefore: now
            )
        }
        let firstOwner = try store.claimDueJobs(
            resourceClass: .embedding,
            capacity: 2,
            now: now,
            owner: "coordinator-a",
            leaseDuration: 60
        )
        XCTAssertEqual(firstOwner.count, 2)
        XCTAssertTrue(try store.claimDueJobs(
            resourceClass: .embedding,
            capacity: 2,
            now: now,
            owner: "coordinator-b",
            leaseDuration: 60
        ).isEmpty)

        let recovered = try store.claimDueJobs(
            resourceClass: .embedding,
            capacity: 2,
            now: now.addingTimeInterval(61),
            owner: "coordinator-b",
            leaseDuration: 60
        )
        XCTAssertEqual(recovered.count, 2)
        for expired in firstOwner {
            XCTAssertEqual(try store.job(id: expired.id)?.state, .retryScheduled)
        }
    }

    func testIllegalTransitionsRejectInProduction() throws {
        _ = try store.ensureJob(makeDesired(key: "transition-guard"))
        let pending = try XCTUnwrap(store.job(idempotencyKey: "transition-guard"))
        XCTAssertThrowsError(try store.markRunning(id: pending.id, leaseToken: UUID()))

        let claimed = try XCTUnwrap(try store.claimDueJobs(
            resourceClass: .embedding,
            capacity: 1,
            now: Date(),
            owner: "transition-test",
            leaseDuration: 60
        ).first)
        let token = try XCTUnwrap(claimed.leaseToken)
        XCTAssertThrowsError(try store.recordExternalOperation(
            id: claimed.id,
            leaseToken: UUID(),
            provider: "fake",
            externalID: "late",
            state: "submitted"
        ))
        try store.markRunning(id: claimed.id, leaseToken: token)
        XCTAssertThrowsError(try store.complete(
            id: claimed.id,
            leaseToken: UUID(),
            outputVersion: "wrong"
        ))
        try store.complete(id: claimed.id, leaseToken: token, outputVersion: "verified")
        XCTAssertThrowsError(try store.complete(
            id: claimed.id,
            leaseToken: token,
            outputVersion: "duplicate"
        ))
    }

    func testExpiredLeaseIsReclaimedAndLateArtifactCommitIsFenced() throws {
        let now = Date(timeIntervalSince1970: 1_000)
        let subject = UUID()
        _ = try store.ensureJob(makeDesired(key: "index:v1", subject: subject), notBefore: now)
        let first = try XCTUnwrap(try store.claimDueJobs(
            resourceClass: .embedding, capacity: 1, now: now,
            owner: "first", leaseDuration: 1
        ).first)
        let firstToken = try XCTUnwrap(first.leaseToken)
        try store.markRunning(id: first.id, leaseToken: firstToken, now: now)

        try store.reclaimExpiredLeases(now: now.addingTimeInterval(2))
        let second = try XCTUnwrap(try store.claimDueJobs(
            resourceClass: .embedding, capacity: 1, now: now.addingTimeInterval(2),
            owner: "second", leaseDuration: 60
        ).first)
        let secondToken = try XCTUnwrap(second.leaseToken)
        XCTAssertNotEqual(firstToken, secondToken)
        XCTAssertEqual(second.attempt, 2)

        let artifacts = ArtifactRepository(fileURL: fileURL)
        let stale = artifact(subject: subject, output: "old")
        XCTAssertThrowsError(try artifacts.commit(
            stale, completingJobID: first.id, leaseToken: firstToken
        )) { error in
            guard case JobStoreError.transitionRejected = error else {
                return XCTFail("Unexpected error: \(error)")
            }
        }
        XCTAssertNil(try artifacts.current(kind: .semanticIndex, subjectID: subject))

        try artifacts.commit(
            artifact(subject: subject, output: "new"),
            completingJobID: second.id,
            leaseToken: secondToken
        )
        XCTAssertEqual(
            try artifacts.current(kind: .semanticIndex, subjectID: subject)?.outputVersion,
            "new"
        )
        XCTAssertEqual(try store.allJobs().first?.state, .succeeded)
    }

    func testInterruptedFinalAttemptBecomesPermanentInsteadOfZombieRetry() throws {
        let now = Date(timeIntervalSince1970: 5_000)
        _ = try store.ensureJob(DesiredJob(
            idempotencyKey: "final-attempt",
            kind: .metadataIndex,
            subjectID: UUID(),
            inputVersion: "v1",
            resourceClass: .embedding,
            maxAttempts: 1
        ), notBefore: now)
        let job = try XCTUnwrap(try store.claimDueJobs(
            resourceClass: .embedding,
            capacity: 1,
            now: now,
            owner: "doomed",
            leaseDuration: 1
        ).first)
        try store.markRunning(
            id: job.id,
            leaseToken: try XCTUnwrap(job.leaseToken),
            now: now
        )

        try store.reclaimExpiredLeases(now: now.addingTimeInterval(2))

        let exhausted = try XCTUnwrap(store.job(idempotencyKey: "final-attempt"))
        XCTAssertEqual(exhausted.state, .failedPermanent)
        XCTAssertFalse(exhausted.state.isActive)
        XCTAssertTrue(try store.claimDueJobs(
            resourceClass: .embedding,
            capacity: 1,
            now: now.addingTimeInterval(3),
            owner: "never",
            leaseDuration: 1
        ).isEmpty)
    }

    func testCoordinatorRespectsResourceLaneCapacityAcrossHundredsOfJobs() async throws {
        let desired = (0..<240).map { makeDesired(key: "stress-\($0)") }
        XCTAssertEqual(try store.ensureJobs(desired), 240)
        let probe = ConcurrencyProbe(delay: .milliseconds(2))
        let coordinator = WorkCoordinator(
            jobStore: store,
            executors: [.metadataIndex: probe],
            capacities: [.embedding: 7],
            leaseDuration: 3_600,
            baseBackoff: 0.01
        )

        await coordinator.drainDueJobs()

        let snapshot = await probe.snapshot()
        XCTAssertEqual(snapshot.completed, 240)
        XCTAssertLessThanOrEqual(snapshot.maximumActive, 7)
        XCTAssertEqual(Set(try store.allJobs().map(\.state)), [.succeeded])
    }

    func testEnsureJobsBatchRollsBackCompletelyWhenInterrupted() throws {
        struct Interrupted: Error {}
        let desired = (0..<3).map { makeDesired(key: "atomic-batch-\($0)") }

        XCTAssertThrowsError(try store.ensureJobs(desired) { index in
            if index == 0 { throw Interrupted() }
        }) { error in
            XCTAssertTrue(error is Interrupted)
        }

        XCTAssertTrue(try store.allJobs().isEmpty)
        XCTAssertEqual(try store.ensureJobs(desired), 3)
    }

    func testMixedBacklogRespectsEverySupportedResourceLaneIndependently() async throws {
        let lanes: [(WorkJobKind, WorkResourceClass, Int)] = [
            (.feedDiscovery, .planning, 1),
            (.metadataIndex, .embedding, 4),
            (.scheduledAgentRun, .scheduledAgent, 2),
            (.newEpisodeNotification, .notification, 5),
        ]
        var desired: [DesiredJob] = []
        for (kind, resource, _) in lanes {
            for index in 0..<30 {
                desired.append(makeDesired(
                    key: "mixed:\(resource.rawValue):\(index)",
                    kind: kind,
                    resource: resource
                ))
            }
        }
        XCTAssertEqual(try store.ensureJobs(desired), desired.count)
        let probe = LaneConcurrencyProbe(delay: .milliseconds(2))
        let executors = Dictionary(
            uniqueKeysWithValues: Set(lanes.map(\.0)).map {
                ($0, probe as any JobExecutor)
            }
        )
        let capacities = Dictionary(uniqueKeysWithValues: lanes.map { ($0.1, $0.2) })
        let coordinator = WorkCoordinator(
            jobStore: store,
            executors: executors,
            capacities: capacities,
            leaseDuration: 3_600,
            baseBackoff: 0.01
        )

        await coordinator.drainDueJobs()

        let snapshot = await probe.snapshot()
        XCTAssertEqual(snapshot.completed, desired.count)
        for (_, resource, capacity) in lanes {
            XCTAssertLessThanOrEqual(snapshot.maximum[resource, default: 0], capacity)
            XCTAssertGreaterThan(snapshot.maximum[resource, default: 0], 0)
        }
        XCTAssertEqual(Set(try store.allJobs().map(\.state)), [.succeeded])
    }

    func testBlockedOutcomeDoesNotSpinOrReportSuccess() async throws {
        _ = try store.ensureJob(makeDesired(key: "blocked"))
        let executor = OutcomeExecutor(.blocked(reason: JobFailure(
            classification: .missingCredential, message: "key missing"
        )))
        let coordinator = WorkCoordinator(
            jobStore: store,
            executors: [.metadataIndex: executor],
            capacities: [.embedding: 1]
        )

        await coordinator.drainDueJobs()
        await coordinator.signal()
        try await Task.sleep(for: .milliseconds(20))

        let runCount = await executor.runCount
        XCTAssertEqual(runCount, 1)
        let job = try XCTUnwrap(store.job(idempotencyKey: "blocked"))
        XCTAssertEqual(job.state, .blocked)
        XCTAssertNil(job.outputVersion)
    }

    func testCancelledOutcomeIsTerminalAndDoesNotRetry() async throws {
        _ = try store.ensureJob(makeDesired(key: "cancelled"))
        let executor = OutcomeExecutor(.cancelled)
        let coordinator = WorkCoordinator(
            jobStore: store,
            executors: [.metadataIndex: executor],
            capacities: [.embedding: 1]
        )

        await coordinator.drainDueJobs()
        XCTAssertEqual(try store.job(idempotencyKey: "cancelled")?.state, .cancelled)
        await coordinator.signal()
        try await Task.sleep(for: .milliseconds(20))
        let runCount = await executor.runCount
        XCTAssertEqual(runCount, 1)
    }

    func testCoordinatorCancellationRetriesOwedWorkInsteadOfConsumingIt() async throws {
        _ = try store.ensureJob(makeDesired(key: "background-expired"))
        let executor = CancellableExecutor()
        let coordinator = WorkCoordinator(
            jobStore: store,
            executors: [.metadataIndex: executor],
            capacities: [.embedding: 1],
            leaseDuration: 60
        )
        await coordinator.start()
        while !(await executor.hasStarted) { await Task.yield() }

        await coordinator.cancelActive()
        try await Task.sleep(for: .milliseconds(50))

        let interrupted = try XCTUnwrap(store.job(idempotencyKey: "background-expired"))
        XCTAssertEqual(interrupted.state, .retryScheduled)
        XCTAssertEqual(interrupted.lastErrorClass, .cancelled)
        XCTAssertNil(interrupted.outputVersion)
    }

    func testExplicitRearmPreservesCanonicalIdentityAfterCancellation() throws {
        let subject = UUID()
        let desired = makeDesired(
            key: "metadata:\(subject):v1",
            subject: subject,
            kind: .metadataIndex,
            resource: .embedding
        )
        XCTAssertTrue(try store.ensureJob(desired))
        let original = try XCTUnwrap(store.job(idempotencyKey: desired.idempotencyKey))

        try store.cancelActiveJobs(kind: .metadataIndex, subjectID: subject)
        XCTAssertEqual(try store.job(idempotencyKey: desired.idempotencyKey)?.state, .cancelled)

        try store.rearmJob(idempotencyKey: desired.idempotencyKey)
        let rearmed = try XCTUnwrap(store.job(idempotencyKey: desired.idempotencyKey))
        XCTAssertEqual(rearmed.id, original.id)
        XCTAssertEqual(rearmed.state, .pending)
        XCTAssertEqual(rearmed.attempt, 0)
        XCTAssertNil(rearmed.leaseToken)
    }

    private func makeDesired(
        key: String,
        subject: UUID = UUID(),
        version: String = "v1",
        kind: WorkJobKind = .metadataIndex,
        priority: Int = 0,
        resource: WorkResourceClass = .embedding
    ) -> DesiredJob {
        DesiredJob(
            idempotencyKey: key, kind: kind, subjectID: subject,
            inputVersion: version, priority: priority, resourceClass: resource
        )
    }

    private func artifact(subject: UUID, output: String) -> ArtifactRecord {
        ArtifactRecord(
            kind: .semanticIndex, subjectID: subject,
            inputVersion: "v1", outputVersion: output,
            contentHash: output, location: nil, origin: "test",
            schemaVersion: 1, integrity: .available, verifiedAt: Date()
        )
    }
}

private actor ConcurrencyProbe: JobExecutor {
    private let delay: Duration
    private var active = 0
    private var maximumActive = 0
    private var completed = 0

    init(delay: Duration) { self.delay = delay }

    func run(_ context: JobAttemptContext) async throws -> JobOutcome {
        active += 1
        maximumActive = max(maximumActive, active)
        if delay != .zero { try await Task.sleep(for: delay) }
        active -= 1
        completed += 1
        return .succeeded(outputVersion: context.job.inputVersion)
    }

    func snapshot() -> (completed: Int, maximumActive: Int) {
        (completed, maximumActive)
    }
}

private actor OutcomeExecutor: JobExecutor {
    let outcome: JobOutcome
    private(set) var runCount = 0

    init(_ outcome: JobOutcome) { self.outcome = outcome }

    func run(_ context: JobAttemptContext) async throws -> JobOutcome {
        runCount += 1
        return outcome
    }
}

private actor LaneConcurrencyProbe: JobExecutor {
    private let delay: Duration
    private var active: [WorkResourceClass: Int] = [:]
    private var maximum: [WorkResourceClass: Int] = [:]
    private var completed = 0

    init(delay: Duration) { self.delay = delay }

    func run(_ context: JobAttemptContext) async throws -> JobOutcome {
        let resource = context.job.resourceClass
        active[resource, default: 0] += 1
        maximum[resource] = max(
            maximum[resource, default: 0],
            active[resource, default: 0]
        )
        if delay != .zero { try await Task.sleep(for: delay) }
        active[resource, default: 0] -= 1
        completed += 1
        return .succeeded(outputVersion: context.job.inputVersion)
    }

    func snapshot() -> (
        completed: Int,
        maximum: [WorkResourceClass: Int]
    ) {
        (completed, maximum)
    }
}

private actor CancellableExecutor: JobExecutor {
    private(set) var hasStarted = false

    func run(_ context: JobAttemptContext) async throws -> JobOutcome {
        hasStarted = true
        try await Task.sleep(for: .seconds(30))
        return .succeeded(outputVersion: context.job.inputVersion)
    }
}
