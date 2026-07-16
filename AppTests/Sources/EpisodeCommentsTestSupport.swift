import Foundation
@testable import Podcastr

final class RepositoryHarness: @unchecked Sendable {
    let repository: HarnessRepository
    let observationContinuation: AsyncThrowingStream<EpisodeCommentSnapshot, any Error>.Continuation
    let receiptContinuation: AsyncStream<EpisodeCommentWriteStatus>.Continuation
    private let state: HarnessState
    private let publishGate: AsyncGate?
    private let reattachGate: AsyncGate?

    init(blockPublish: Bool = false, blockReattach: Bool = false) {
        var observationContinuation: AsyncThrowingStream<EpisodeCommentSnapshot, any Error>.Continuation!
        let observations = AsyncThrowingStream<EpisodeCommentSnapshot, any Error> {
            observationContinuation = $0
        }
        var receiptContinuation: AsyncStream<EpisodeCommentWriteStatus>.Continuation!
        let statuses = AsyncStream<EpisodeCommentWriteStatus> { receiptContinuation = $0 }
        let state = HarnessState()
        let publishGate = blockPublish ? AsyncGate() : nil
        let reattachGate = blockReattach ? AsyncGate() : nil
        self.state = state
        self.publishGate = publishGate
        self.reattachGate = reattachGate
        self.observationContinuation = observationContinuation
        self.receiptContinuation = receiptContinuation
        self.repository = HarnessRepository(
            state: state,
            observation: EpisodeCommentObservation(updates: observations) {
                state.lock.withLock { state.observationCancelled = true }
            },
            receipt: EpisodeCommentReceipt(id: 42, statuses: statuses),
            publishGate: publishGate,
            reattachGate: reattachGate
        )
    }

    var observeCount: Int { state.lock.withLock { state.observeCount } }
    var observationCancelled: Bool { state.lock.withLock { state.observationCancelled } }
    var reattachedIDs: [UInt64] { state.lock.withLock { state.reattachedIDs } }
    var publishCount: Int { state.lock.withLock { state.publishCount } }
    var publishedContents: [String] { state.lock.withLock { state.publishedContents } }

    func releasePublish() { publishGate?.open() }
    func releaseReattach() { reattachGate?.open() }
}

final class HarnessState: @unchecked Sendable {
    let lock = NSLock()
    var observeCount = 0
    var observationCancelled = false
    var reattachedIDs: [UInt64] = []
    var publishCount = 0
    var publishedContents: [String] = []
}

struct HarnessRepository: EpisodeCommentsRepository {
    let state: HarnessState
    let observation: EpisodeCommentObservation
    let receipt: EpisodeCommentReceipt
    let publishGate: AsyncGate?
    let reattachGate: AsyncGate?
    let availability = EpisodeCommentsAvailability.available

    func activeAuthorPubkey() async throws -> String? { String(repeating: "b", count: 64) }

    func observe(target: CommentTarget) async throws -> EpisodeCommentObservation {
        state.lock.withLock { state.observeCount += 1 }
        return observation
    }

    func publish(content: String, target: CommentTarget) async throws -> EpisodeCommentReceipt {
        state.lock.withLock {
            state.publishCount += 1
            state.publishedContents.append(content)
        }
        await publishGate?.wait()
        return receipt
    }

    func reattachReceipt(id: UInt64) async throws -> EpisodeCommentReceiptReattachment {
        state.lock.withLock { state.reattachedIDs.append(id) }
        await reattachGate?.wait()
        return .attached(receipt)
    }
}

final class AsyncGate: @unchecked Sendable {
    private let lock = NSLock()
    private var isOpen = false
    private var waiters: [CheckedContinuation<Void, Never>] = []

    func wait() async {
        await withCheckedContinuation { continuation in
            let resumeNow = lock.withLock {
                guard !isOpen else { return true }
                waiters.append(continuation)
                return false
            }
            if resumeNow { continuation.resume() }
        }
    }

    func open() {
        let pending = lock.withLock {
            isOpen = true
            let pending = waiters
            waiters.removeAll()
            return pending
        }
        pending.forEach { $0.resume() }
    }
}

final class MemoryReceiptStore: EpisodeCommentReceiptStore, @unchecked Sendable {
    private let lock = NSLock()
    private var values: [PendingEpisodeCommentReceipt]

    init(records: [PendingEpisodeCommentReceipt] = []) { values = records }

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

    func removeAll() { lock.withLock { values.removeAll() } }
}

struct FailingSaveReceiptStore: EpisodeCommentReceiptStore {
    func records(for target: CommentTarget) -> [PendingEpisodeCommentReceipt] { [] }
    func save(_ record: PendingEpisodeCommentReceipt) throws {
        throw EpisodeCommentReceiptStoreError.unreadable
    }
    func remove(receiptID: UInt64) {}
    func removeAll() {}
}

final class FailingEventCorrelationReceiptStore: EpisodeCommentReceiptStore, @unchecked Sendable {
    private let lock = NSLock()
    private var values: [PendingEpisodeCommentReceipt] = []

    func records(for target: CommentTarget) -> [PendingEpisodeCommentReceipt] {
        lock.withLock { values.filter { $0.target == target } }
    }

    func save(_ record: PendingEpisodeCommentReceipt) throws {
        guard record.eventID == nil else { throw EpisodeCommentReceiptStoreError.unreadable }
        lock.withLock {
            values.removeAll { $0.receiptID == record.receiptID }
            values.append(record)
        }
    }

    func remove(receiptID: UInt64) {
        lock.withLock { values.removeAll { $0.receiptID == receiptID } }
    }

    func removeAll() { lock.withLock { values.removeAll() } }
}

final class InitiallyFailingReceiptStore: EpisodeCommentReceiptStore, @unchecked Sendable {
    private let lock = NSLock()
    private var didFail = false
    private var values: [PendingEpisodeCommentReceipt] = []

    func records(for target: CommentTarget) -> [PendingEpisodeCommentReceipt] {
        lock.withLock { values.filter { $0.target == target } }
    }

    func save(_ record: PendingEpisodeCommentReceipt) throws {
        try lock.withLock {
            guard didFail else {
                didFail = true
                throw EpisodeCommentReceiptStoreError.unreadable
            }
            values.removeAll { $0.receiptID == record.receiptID }
            values.append(record)
        }
    }

    func remove(receiptID: UInt64) {
        lock.withLock { values.removeAll { $0.receiptID == receiptID } }
    }

    func removeAll() { lock.withLock { values.removeAll() } }
}
