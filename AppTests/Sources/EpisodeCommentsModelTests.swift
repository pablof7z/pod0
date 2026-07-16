import Foundation
import XCTest
@testable import Podcastr

@MainActor
final class EpisodeCommentsModelTests: XCTestCase {
    private let target = CommentTarget.episode(guid: "episode-guid")

    func testObservationUsesAuthoritativeSnapshotsAndCancelsDemand() async throws {
        let harness = RepositoryHarness()
        let model = EpisodeCommentsModel(repository: harness.repository, receiptStore: MemoryReceiptStore())
        let task = Task { await model.observe(target: target) }
        await eventually { harness.observeCount == 1 }

        let older = comment(id: "old", createdAt: 1)
        let newer = comment(id: "new", createdAt: 2)
        harness.observationContinuation.yield(EpisodeCommentSnapshot(
            comments: [older, newer],
            acquisition: EpisodeCommentAcquisition(
                sourceCount: 2,
                connectedSourceCount: 1,
                hasShortfall: true,
                lastReconciledAt: Date(timeIntervalSince1970: 10)
            )
        ))
        await eventually { model.comments.count == 2 }

        XCTAssertEqual(model.comments.map(\.id), ["new", "old"])
        XCTAssertEqual(model.acquisition.connectedSourceCount, 1)
        task.cancel()
        await task.value
        XCTAssertTrue(harness.observationCancelled)
    }

    func testSubmitShowsReceiptFactsAndNeverOptimisticallyAddsCanonicalComment() async throws {
        let harness = RepositoryHarness()
        let store = MemoryReceiptStore()
        let model = EpisodeCommentsModel(repository: harness.repository, receiptStore: store)
        let observeTask = Task { await model.observe(target: target) }
        await eventually { model.activeAuthorPubkey != nil }

        model.draft = "  Hello, listeners.  "
        await model.submit(target: target)

        XCTAssertTrue(model.comments.isEmpty)
        XCTAssertEqual(model.outgoing.first?.phase, .queued)
        XCTAssertEqual(store.records(for: target).map(\.receiptID), [42])
        XCTAssertNil(store.records(for: target).first?.eventID)

        harness.receiptContinuation.yield(.sent(relay: "wss://relay.example"))
        await eventually { model.outgoing.first?.phase == .awaitingConfirmation }
        XCTAssertTrue(model.comments.isEmpty, "A sent frame is not a canonical observed comment.")

        harness.receiptContinuation.yield(.acknowledged(relay: "wss://relay.example"))
        harness.receiptContinuation.finish()
        await eventually { model.outgoing.first?.phase == .published(relayCount: 1) }
        XCTAssertTrue(store.records(for: target).isEmpty)

        observeTask.cancel()
        await observeTask.value
    }

    func testSignedReceiptDisappearsOnlyAfterCanonicalObservation() async throws {
        let harness = RepositoryHarness()
        let store = MemoryReceiptStore()
        let model = EpisodeCommentsModel(repository: harness.repository, receiptStore: store)
        let task = Task { await model.observe(target: target) }
        await eventually { harness.observeCount == 1 }
        model.draft = "Canonical me"
        await model.submit(target: target)

        harness.receiptContinuation.yield(.signed(eventID: "event-42"))
        await eventually { model.outgoing.first?.phase == .signed }
        XCTAssertEqual(model.outgoing.count, 1)
        XCTAssertEqual(store.records(for: target).first?.eventID, "event-42")

        harness.observationContinuation.yield(EpisodeCommentSnapshot(
            comments: [comment(id: "event-42", createdAt: 3)],
            acquisition: .starting
        ))
        await eventually { model.comments.first?.id == "event-42" && model.outgoing.isEmpty }

        harness.receiptContinuation.finish()
        task.cancel()
        await task.value
    }

    func testRestartReattachesPersistedReceipt() async throws {
        let harness = RepositoryHarness()
        let record = PendingEpisodeCommentReceipt(
            receiptID: 42,
            target: target,
            eventID: "event-42",
            submittedAt: Date(timeIntervalSince1970: 5)
        )
        let store = MemoryReceiptStore(records: [record])
        let model = EpisodeCommentsModel(repository: harness.repository, receiptStore: store)
        let task = Task { await model.observe(target: target) }

        await eventually { harness.reattachedIDs == [42] }
        XCTAssertEqual(model.outgoing.first?.phase, .queued)

        harness.receiptContinuation.yield(.acknowledged(relay: "wss://relay.example"))
        harness.receiptContinuation.finish()
        await eventually { store.records(for: target).isEmpty }

        task.cancel()
        await task.value
    }

    func testReceiptRollupKeepsDistinctActionableStates() async throws {
        let harness = RepositoryHarness()
        let model = EpisodeCommentsModel(repository: harness.repository, receiptStore: MemoryReceiptStore())
        let task = Task { await model.observe(target: target) }
        await eventually { model.activeAuthorPubkey != nil }
        model.draft = "Status test"
        await model.submit(target: target)

        harness.receiptContinuation.yield(.awaitingAuth(relay: "wss://relay.example"))
        await eventually { model.outgoing.first?.phase == .awaitingRelayAuthorization }

        let eligibleAt = Date(timeIntervalSince1970: 10)
        harness.receiptContinuation.yield(
            .retryEligible(relay: "wss://relay.example", eligibleAt: eligibleAt)
        )
        await eventually { model.outgoing.first?.phase == .retrying(eligibleAt: eligibleAt) }

        harness.receiptContinuation.yield(.outcomeUnknown(relay: "wss://relay.example"))
        await eventually {
            model.outgoing.first?.phase == .deliveryUnknown("wss://relay.example")
        }

        task.cancel()
        await task.value
    }

    private func comment(id: String, createdAt: TimeInterval) -> EpisodeComment {
        EpisodeComment(
            id: id,
            target: target,
            authorPubkeyHex: String(repeating: "a", count: 64),
            content: id,
            createdAt: Date(timeIntervalSince1970: createdAt)
        )
    }

    private func eventually(
        _ condition: @MainActor () -> Bool,
        file: StaticString = #filePath,
        line: UInt = #line
    ) async {
        for _ in 0..<100 where !condition() { await Task.yield() }
        XCTAssertTrue(condition(), file: file, line: line)
    }
}

private final class RepositoryHarness: @unchecked Sendable {
    let repository: HarnessRepository
    let observationContinuation: AsyncThrowingStream<EpisodeCommentSnapshot, any Error>.Continuation
    let receiptContinuation: AsyncStream<EpisodeCommentWriteStatus>.Continuation
    private let state: HarnessState

    init() {
        var observationContinuation: AsyncThrowingStream<EpisodeCommentSnapshot, any Error>.Continuation!
        let observations = AsyncThrowingStream<EpisodeCommentSnapshot, any Error> {
            observationContinuation = $0
        }
        var receiptContinuation: AsyncStream<EpisodeCommentWriteStatus>.Continuation!
        let statuses = AsyncStream<EpisodeCommentWriteStatus> { receiptContinuation = $0 }
        let state = HarnessState()
        self.state = state
        self.observationContinuation = observationContinuation
        self.receiptContinuation = receiptContinuation
        self.repository = HarnessRepository(
            state: state,
            observation: EpisodeCommentObservation(updates: observations) {
                state.lock.withLock { state.observationCancelled = true }
            },
            receipt: EpisodeCommentReceipt(id: 42, statuses: statuses)
        )
    }

    var observeCount: Int { state.lock.withLock { state.observeCount } }
    var observationCancelled: Bool { state.lock.withLock { state.observationCancelled } }
    var reattachedIDs: [UInt64] { state.lock.withLock { state.reattachedIDs } }
}

private final class HarnessState: @unchecked Sendable {
    let lock = NSLock()
    var observeCount = 0
    var observationCancelled = false
    var reattachedIDs: [UInt64] = []
}

private struct HarnessRepository: EpisodeCommentsRepository {
    let state: HarnessState
    let observation: EpisodeCommentObservation
    let receipt: EpisodeCommentReceipt

    func activeAuthorPubkey() async throws -> String? { String(repeating: "b", count: 64) }

    func observe(target: CommentTarget) async throws -> EpisodeCommentObservation {
        state.lock.withLock { state.observeCount += 1 }
        return observation
    }

    func publish(content: String, target: CommentTarget) async throws -> EpisodeCommentReceipt {
        receipt
    }

    func reattachReceipt(id: UInt64) async throws -> EpisodeCommentReceiptReattachment {
        state.lock.withLock { state.reattachedIDs.append(id) }
        return .attached(receipt)
    }
}

private final class MemoryReceiptStore: EpisodeCommentReceiptStore, @unchecked Sendable {
    private let lock = NSLock()
    private var values: [PendingEpisodeCommentReceipt]

    init(records: [PendingEpisodeCommentReceipt] = []) {
        values = records
    }

    func records(for target: CommentTarget) -> [PendingEpisodeCommentReceipt] {
        lock.withLock { values.filter { $0.target == target } }
    }

    func save(_ record: PendingEpisodeCommentReceipt) {
        lock.withLock {
            values.removeAll { $0.receiptID == record.receiptID }
            values.append(record)
        }
    }

    func remove(receiptID: UInt64) {
        lock.withLock { values.removeAll { $0.receiptID == receiptID } }
    }
}
