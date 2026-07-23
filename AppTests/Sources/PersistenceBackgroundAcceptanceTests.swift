import Foundation
import XCTest
@testable import Podcastr

@MainActor
final class PersistenceBackgroundAcceptanceTests: XCTestCase {
    func testSaveTransfersOwnershipBeforeAsynchronousDrainSignal() async throws {
        let url = AppStateTestSupport.uniqueTempFileURL()
        defer { AppStateTestSupport.disposeIsolatedStore(at: url) }
        let gate = RevisionEnqueueGate(blockingRevision: 1)
        let persistence = Persistence(
            fileURL: url,
            writeMode: .background,
            beforeBackgroundEnqueue: { revision in await gate.waitIfBlocked(revision) }
        )
        var first = AppState()
        first.settings.hasCompletedOnboarding = false
        var second = first
        second.settings.hasCompletedOnboarding = true
        let occurrence = DesiredJob(
            idempotencyKey: "scheduled:reversed-save",
            kind: .scheduledAgentRun,
            subjectID: UUID(),
            inputVersion: "scheduled:reversed-save",
            occurrenceID: "scheduled:reversed-save",
            resourceClass: .scheduledAgent
        )

        let firstRevision = persistence.save(first, ensuring: [occurrence])
        XCTAssertEqual(firstRevision, 1)
        XCTAssertEqual(persistence.latestSynchronouslyAcceptedRevision, firstRevision)
        await gate.waitUntilEntered()

        let secondRevision = persistence.save(second)
        XCTAssertEqual(secondRevision, 2)
        XCTAssertEqual(persistence.latestSynchronouslyAcceptedRevision, secondRevision)
        let wroteSecond = await persistence.waitUntilWritten(secondRevision)
        XCTAssertTrue(wroteSecond)
        await gate.release()

        let loaded = try Persistence(fileURL: url).load()
        XCTAssertTrue(loaded.settings.hasCompletedOnboarding)
        XCTAssertEqual(loaded.persistenceGeneration, secondRevision)
        let jobStore = JobStore(fileURL: Persistence.episodeStoreURL(for: url))
        XCTAssertEqual(
            try jobStore.job(idempotencyKey: occurrence.idempotencyKey)?.state,
            .pending
        )
        XCTAssertEqual(
            try jobStore.allJobs().filter {
                $0.idempotencyKey == occurrence.idempotencyKey
            }.count,
            1
        )
    }
}

private actor RevisionEnqueueGate {
    let blockingRevision: UInt64
    private var entered = false
    private var drainWaiter: CheckedContinuation<Void, Never>?
    private var entryWaiters: [CheckedContinuation<Void, Never>] = []

    init(blockingRevision: UInt64) {
        self.blockingRevision = blockingRevision
    }

    func waitIfBlocked(_ revision: UInt64) async {
        guard revision == blockingRevision else { return }
        entered = true
        for waiter in entryWaiters { waiter.resume() }
        entryWaiters.removeAll()
        await withCheckedContinuation { drainWaiter = $0 }
    }

    func waitUntilEntered() async {
        guard !entered else { return }
        await withCheckedContinuation { entryWaiters.append($0) }
    }

    func release() {
        drainWaiter?.resume()
        drainWaiter = nil
    }
}
