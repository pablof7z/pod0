import Foundation

actor PersistenceBackgroundWriter {
    private struct PendingSnapshot {
        let revision: UInt64
        let state: AppState
        let jobs: [DesiredJob]
    }

    private var pending: PendingSnapshot?
    private var latestAcceptedSnapshot: PendingSnapshot?
    private var isDraining = false
    private var lastAcceptedRevision: UInt64 = 0
    private var lastWrittenRevision: UInt64 = 0
    private var failedRevisions: Set<UInt64> = []
    private var uncommittedJobs: [String: DesiredJob] = [:]
    private var waiters: [(revision: UInt64, continuation: CheckedContinuation<Bool, Never>)] = []

    func enqueue(
        revision: UInt64,
        state: AppState,
        jobs: [DesiredJob],
        persistence: Persistence
    ) {
        for job in jobs { uncommittedJobs[job.idempotencyKey] = job }
        if revision > lastAcceptedRevision {
            lastAcceptedRevision = revision
            latestAcceptedSnapshot = PendingSnapshot(
                revision: revision,
                state: state,
                jobs: []
            )
        } else if jobs.isEmpty {
            return
        }
        guard let latestAcceptedSnapshot else { return }
        let mergedJobs = uncommittedJobs.values.sorted {
            $0.idempotencyKey < $1.idempotencyKey
        }
        pending = PendingSnapshot(
            revision: latestAcceptedSnapshot.revision,
            state: latestAcceptedSnapshot.state,
            jobs: mergedJobs
        )
        guard !isDraining else { return }
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
        while let snapshot = pending {
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
