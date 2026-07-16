import Foundation
import XCTest
@testable import Podcastr

@MainActor
final class EpisodeCommentsModelTests: XCTestCase {
    private let target = CommentTarget.episode(guid: "episode-guid")

    func testUnavailableProviderExplainsFailClosedState() {
        let repository = UnavailableEpisodeCommentsRepository()
        guard case .blocked(let message) = repository.availability else {
            return XCTFail("Expected comments to remain blocked")
        }
        XCTAssertTrue(message.contains("paused"))
        XCTAssertTrue(message.contains("won't use the old unverified relay path"))
    }

    func testUnavailableProviderRefusesToOpenAReadObservation() async {
        let repository = UnavailableEpisodeCommentsRepository()

        do {
            _ = try await repository.observe(target: target)
            XCTFail("A missing typed NMP comment surface must fail closed")
        } catch let error as EpisodeCommentsRepositoryError {
            guard case .unavailable = error else {
                return XCTFail("Unexpected repository error: \(error)")
            }
        } catch {
            XCTFail("Unexpected error type: \(error)")
        }
    }

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

    func testObservationUsesEventIDAsStableTieBreaker() async {
        let harness = RepositoryHarness()
        let model = EpisodeCommentsModel(repository: harness.repository, receiptStore: MemoryReceiptStore())
        let task = Task { await model.observe(target: target) }
        await eventually { harness.observeCount == 1 }
        let timestamp = Date(timeIntervalSince1970: 1)

        harness.observationContinuation.yield(EpisodeCommentSnapshot(
            comments: [comment(id: "z", createdAt: timestamp), comment(id: "a", createdAt: timestamp)],
            acquisition: .starting
        ))

        await eventually { model.comments.count == 2 }
        XCTAssertEqual(model.comments.map(\.id), ["a", "z"])
        task.cancel()
        await task.value
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
        XCTAssertEqual(model.draft, "  Hello, listeners.  ")
        XCTAssertFalse(model.canSubmit)
        XCTAssertEqual(model.outgoing.first?.phase, .queued)
        XCTAssertEqual(store.records(for: target).map(\.receiptID), [42])

        harness.receiptContinuation.yield(.accepted)
        await eventually { model.draft.isEmpty }
        harness.receiptContinuation.yield(.sent(relay: "wss://relay.example"))
        await eventually { model.outgoing.first?.phase == .awaitingConfirmation }
        XCTAssertTrue(model.comments.isEmpty)

        harness.receiptContinuation.yield(.acknowledged(relay: "wss://relay.example"))
        harness.receiptContinuation.finish()
        await eventually {
            model.outgoing.first?.phase == .published(
                confirmedRelayCount: 1,
                unconfirmedRelayCount: 0,
                pendingRelayCount: 0
            )
        }
        await eventually { store.records(for: target).isEmpty }
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

    func testOverlappingObservationsReattachEachReceiptOnlyOnce() async {
        let harness = RepositoryHarness(blockReattach: true)
        let store = MemoryReceiptStore(records: [pendingReceipt()])
        let model = EpisodeCommentsModel(repository: harness.repository, receiptStore: store)
        let first = Task { await model.observe(target: target) }
        await eventually { harness.reattachedIDs == [42] }

        let second = Task { await model.observe(target: target) }
        await eventually { harness.observeCount == 1 }
        XCTAssertEqual(harness.reattachedIDs, [42])

        harness.releaseReattach()
        await eventually { harness.observeCount == 2 }
        harness.receiptContinuation.finish()
        first.cancel()
        second.cancel()
        await first.value
        await second.value
    }

    func testCancellationDuringReattachmentDoesNotOpenReadObservation() async {
        let harness = RepositoryHarness(blockReattach: true)
        let store = MemoryReceiptStore(records: [pendingReceipt()])
        let model = EpisodeCommentsModel(repository: harness.repository, receiptStore: store)
        let task = Task { await model.observe(target: target) }
        await eventually { harness.reattachedIDs == [42] }

        task.cancel()
        harness.releaseReattach()
        await task.value

        XCTAssertEqual(harness.observeCount, 0)
        XCTAssertFalse(model.isLoading)
        harness.receiptContinuation.finish()
    }

    private func pendingReceipt() -> PendingEpisodeCommentReceipt {
        PendingEpisodeCommentReceipt(
            receiptID: 42,
            target: target,
            eventID: nil,
            submittedAt: Date(timeIntervalSince1970: 5)
        )
    }

    private func comment(id: String, createdAt: TimeInterval) -> EpisodeComment {
        comment(id: id, createdAt: Date(timeIntervalSince1970: createdAt))
    }

    private func comment(id: String, createdAt: Date) -> EpisodeComment {
        EpisodeComment(
            id: id,
            target: target,
            authorPubkeyHex: String(repeating: "a", count: 64),
            content: id,
            createdAt: createdAt
        )
    }

    private func eventually(_ condition: @MainActor () -> Bool) async {
        for _ in 0..<100 where !condition() { await Task.yield() }
        XCTAssertTrue(condition())
    }
}
