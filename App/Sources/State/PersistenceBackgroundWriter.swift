import Foundation
import os

private struct PersistencePendingSnapshot {
    let revision: UInt64
    let state: AppState
    let jobs: [DesiredJob]
}

private final class PersistenceBackgroundInbox: @unchecked Sendable {
    private struct State {
        var latestSnapshot: PersistencePendingSnapshot?
        var latestAcceptedRevision: UInt64 = 0
        var jobs: [String: DesiredJob] = [:]
    }

    private let state = OSAllocatedUnfairLock<State>(initialState: State())

    func accept(revision: UInt64, state snapshot: AppState, jobs: [DesiredJob]) {
        state.withLock { state in
            for job in jobs { state.jobs[job.idempotencyKey] = job }
            state.latestAcceptedRevision = max(state.latestAcceptedRevision, revision)
            if revision > (state.latestSnapshot?.revision ?? 0) {
                state.latestSnapshot = PersistencePendingSnapshot(
                    revision: revision,
                    state: snapshot,
                    jobs: []
                )
            }
        }
    }

    func take() -> PersistencePendingSnapshot? {
        state.withLock { state in
            guard let latest = state.latestSnapshot else { return nil }
            let accepted = PersistencePendingSnapshot(
                revision: latest.revision,
                state: latest.state,
                jobs: state.jobs.values.sorted {
                    $0.idempotencyKey < $1.idempotencyKey
                }
            )
            state.latestSnapshot = nil
            state.jobs.removeAll(keepingCapacity: true)
            return accepted
        }
    }

    var latestAcceptedRevision: UInt64 {
        state.withLock { $0.latestAcceptedRevision }
    }
}

actor PersistenceBackgroundWriter {
    nonisolated private let inbox = PersistenceBackgroundInbox()
    private var pending: PersistencePendingSnapshot?
    private var latestAcceptedSnapshot: PersistencePendingSnapshot?
    private var isDraining = false
    private var lastAcceptedRevision: UInt64 = 0
    private var lastWrittenRevision: UInt64 = 0
    private var failedRevisions: Set<UInt64> = []
    private var uncommittedJobs: [String: DesiredJob] = [:]
    private var waiters: [(revision: UInt64, continuation: CheckedContinuation<Bool, Never>)] = []

    nonisolated func accept(
        revision: UInt64,
        state: AppState,
        jobs: [DesiredJob]
    ) {
        inbox.accept(revision: revision, state: state, jobs: jobs)
    }

    nonisolated var latestSynchronouslyAcceptedRevision: UInt64 {
        inbox.latestAcceptedRevision
    }

    func start(persistence: Persistence) {
        ingestAcceptedSnapshots()
        guard !isDraining else { return }
        guard pending != nil else { return }
        isDraining = true
        Task { await drain(persistence: persistence) }
    }

    func waitUntilWritten(_ revision: UInt64) async -> Bool {
        if lastWrittenRevision >= revision { return true }
        if failedRevisions.contains(revision) { return false }
        return await withCheckedContinuation { continuation in
            waiters.append((revision, continuation))
        }
    }

    private func drain(persistence: Persistence) async {
        while true {
            ingestAcceptedSnapshots()
            guard let snapshot = pending else { break }
            pending = nil
            let succeeded = await Task.detached(priority: .utility) {
                persistence.write(
                    snapshot.state,
                    revision: snapshot.revision,
                    ensuring: snapshot.jobs
                )
            }.value
            if succeeded {
                lastWrittenRevision = max(lastWrittenRevision, snapshot.revision)
                for job in snapshot.jobs where uncommittedJobs[job.idempotencyKey] == job {
                    uncommittedJobs[job.idempotencyKey] = nil
                }
            } else {
                failedRevisions.insert(snapshot.revision)
            }
            resumeSatisfiedWaiters()
        }
        isDraining = false
    }

    private func ingestAcceptedSnapshots() {
        guard let accepted = inbox.take() else { return }
        for job in accepted.jobs { uncommittedJobs[job.idempotencyKey] = job }
        if accepted.revision > lastAcceptedRevision {
            lastAcceptedRevision = accepted.revision
            latestAcceptedSnapshot = PersistencePendingSnapshot(
                revision: accepted.revision,
                state: accepted.state,
                jobs: []
            )
        }
        guard let latestAcceptedSnapshot else { return }
        pending = PersistencePendingSnapshot(
            revision: latestAcceptedSnapshot.revision,
            state: latestAcceptedSnapshot.state,
            jobs: uncommittedJobs.values.sorted {
                $0.idempotencyKey < $1.idempotencyKey
            }
        )
    }

    private func resumeSatisfiedWaiters() {
        var remaining: [(UInt64, CheckedContinuation<Bool, Never>)] = []
        for waiter in waiters {
            if waiter.revision <= lastWrittenRevision {
                waiter.continuation.resume(returning: true)
            } else if failedRevisions.contains(waiter.revision) {
                waiter.continuation.resume(returning: false)
            } else {
                remaining.append(waiter)
            }
        }
        waiters = remaining
    }
}

extension Persistence {
    /// Test diagnostic proving background ownership transfers before `save`
    /// returns, even if the asynchronous drain signal has not run yet.
    var latestSynchronouslyAcceptedRevision: UInt64 {
        backgroundWriter.latestSynchronouslyAcceptedRevision
    }
}
