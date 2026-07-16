import Foundation
import XCTest
@testable import Podcastr

@MainActor
final class EpisodeCommentReceiptPolicyTests: XCTestCase {
    private let target = CommentTarget.episode(guid: "episode-guid")

    func testAcceptanceDoesNotEraseReplacementDraftOrPublishTwice() async {
        let harness = RepositoryHarness()
        let model = EpisodeCommentsModel(repository: harness.repository, receiptStore: MemoryReceiptStore())
        let task = Task { await model.observe(target: target) }
        await eventually { model.activeAuthorPubkey != nil }

        model.draft = "First draft"
        await model.submit(target: target)
        model.draft = "Next draft"
        await model.submit(target: target)
        XCTAssertEqual(harness.publishCount, 1)

        harness.receiptContinuation.yield(.accepted)
        await eventually { model.canSubmit }
        XCTAssertEqual(model.draft, "Next draft")
        task.cancel()
        await task.value
    }

    func testMixedAckRollupRetainsUnconfirmedAndPendingEvidence() async {
        let harness = RepositoryHarness()
        let model = EpisodeCommentsModel(repository: harness.repository, receiptStore: MemoryReceiptStore())
        let task = Task { await model.observe(target: target) }
        await eventually { model.activeAuthorPubkey != nil }
        model.draft = "Mixed result"
        await model.submit(target: target)

        harness.receiptContinuation.yield(.accepted)
        harness.receiptContinuation.yield(.routed(relays: ["relay-a", "relay-b", "relay-c"]))
        harness.receiptContinuation.yield(.rejected(relay: "relay-b", reason: "blocked"))
        await eventually { model.outgoing.first?.phase == .delivering }
        harness.receiptContinuation.yield(.acknowledged(relay: "relay-a"))
        await eventually {
            model.outgoing.first?.phase == .published(
                confirmedRelayCount: 1,
                unconfirmedRelayCount: 1,
                pendingRelayCount: 1
            )
        }
        XCTAssertEqual(
            model.outgoing.first?.phase.label,
            "Posted: 1 relay confirmed; 1 unconfirmed; 1 still pending"
        )
        task.cancel()
        await task.value
    }

    func testStreamEndBeforeAcceptanceKeepsDraftLockedAndReceiptDurable() async {
        let harness = RepositoryHarness()
        let store = MemoryReceiptStore()
        let model = EpisodeCommentsModel(repository: harness.repository, receiptStore: store)
        let task = Task { await model.observe(target: target) }
        await eventually { model.activeAuthorPubkey != nil }
        model.draft = "Do not duplicate"
        await model.submit(target: target)

        harness.receiptContinuation.finish()
        await eventually {
            guard case .deliveryUnknown(let message) = model.outgoing.first?.phase else {
                return false
            }
            return message.contains("before durable acceptance")
        }
        XCTAssertEqual(model.draft, "Do not duplicate")
        XCTAssertFalse(model.canSubmit)
        XCTAssertEqual(store.records(for: target).map(\.receiptID), [42])
        task.cancel()
        await task.value
    }

    func testFailedRestartAnnotationNeverClearsOrUnlocksDraftOnAcceptance() async {
        let harness = RepositoryHarness()
        let model = EpisodeCommentsModel(
            repository: harness.repository,
            receiptStore: FailingSaveReceiptStore()
        )
        let task = Task { await model.observe(target: target) }
        await eventually { model.activeAuthorPubkey != nil }
        model.draft = "Keep this draft"
        await model.submit(target: target)

        XCTAssertEqual(model.submitError, EpisodeCommentReceiptStoreError.unreadable.localizedDescription)
        harness.receiptContinuation.yield(.accepted)
        await eventually { model.outgoing.first?.phase == .queued }
        XCTAssertEqual(model.draft, "Keep this draft")
        XCTAssertFalse(model.canSubmit)
        XCTAssertEqual(harness.publishCount, 1)

        task.cancel()
        await task.value
    }

    func testActionableReceiptStatesRemainDistinct() async {
        let harness = RepositoryHarness()
        let model = EpisodeCommentsModel(repository: harness.repository, receiptStore: MemoryReceiptStore())
        let task = Task { await model.observe(target: target) }
        await eventually { model.activeAuthorPubkey != nil }
        model.draft = "Status test"
        await model.submit(target: target)

        harness.receiptContinuation.yield(.awaitingAuth(relay: "relay"))
        await eventually { model.outgoing.first?.phase == .awaitingRelayAuthorization }
        let eligibleAt = Date(timeIntervalSince1970: 10)
        harness.receiptContinuation.yield(.retryEligible(relay: "relay", eligibleAt: eligibleAt))
        await eventually { model.outgoing.first?.phase == .retrying(eligibleAt: eligibleAt) }
        harness.receiptContinuation.yield(.outcomeUnknown(relay: "relay"))
        await eventually { model.outgoing.first?.phase == .deliveryUnknown("relay") }
        task.cancel()
        await task.value
    }

    private func eventually(_ condition: @MainActor () -> Bool) async {
        for _ in 0..<100 where !condition() { await Task.yield() }
        XCTAssertTrue(condition())
    }
}

final class EpisodeCommentReceiptRollupTests: XCTestCase {
    func testNoAckTerminalPrecedence() {
        var all = EpisodeCommentReceiptRollup()
        all.apply(.routed(relays: ["reject", "gave-up", "unknown", "persist", "route-persist"]))
        all.apply(.rejected(relay: "reject", reason: "policy"))
        all.apply(.gaveUp(relay: "gave-up"))
        all.apply(.outcomeUnknown(relay: "unknown"))
        all.apply(.persistenceBlocked(relay: "persist"))
        all.apply(.routePersistenceBlocked(relay: "route-persist"))
        guard case .persistenceBlocked = all.phase(streamEnded: false) else {
            return XCTFail("Persistence evidence must have first precedence.")
        }

        var unknown = EpisodeCommentReceiptRollup()
        unknown.apply(.routed(relays: ["reject", "gave-up", "unknown"]))
        unknown.apply(.rejected(relay: "reject", reason: "policy"))
        unknown.apply(.gaveUp(relay: "gave-up"))
        unknown.apply(.outcomeUnknown(relay: "unknown"))
        XCTAssertEqual(unknown.phase(streamEnded: false), .deliveryUnknown("unknown"))

        var gaveUp = EpisodeCommentReceiptRollup()
        gaveUp.apply(.routed(relays: ["reject", "gave-up"]))
        gaveUp.apply(.rejected(relay: "reject", reason: "policy"))
        gaveUp.apply(.gaveUp(relay: "gave-up"))
        XCTAssertEqual(gaveUp.phase(streamEnded: false), .gaveUp("gave-up"))
    }

    func testAllRoutesMustBeTerminalBeforeNoAckRejection() {
        var rollup = EpisodeCommentReceiptRollup()
        rollup.apply(.routed(relays: ["relay-a", "relay-b"]))
        rollup.apply(.rejected(relay: "relay-a", reason: "policy"))
        XCTAssertEqual(rollup.phase(streamEnded: false), .delivering)
        rollup.apply(.rejected(relay: "relay-b", reason: "policy"))
        guard case .rejected = rollup.phase(streamEnded: false) else {
            return XCTFail("All-terminal rejection should be visible.")
        }
    }
}
