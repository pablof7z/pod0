import Foundation
import os.log

protocol JobPostconditionVerifier: Sendable {
    /// Verifies the output and atomically commits both its artifact fact and
    /// the fenced job success marker. Returns false for an invalid output.
    func verifyAndCommit(
        _ job: WorkJob,
        leaseToken: UUID,
        outputVersion: String?
    ) async throws -> Bool
}

actor WorkCoordinator {
    private static let logger = Logger.app("WorkCoordinator")

    let jobStore: JobStore
    private let executors: [WorkJobKind: any JobExecutor]
    private let verifiers: [WorkJobKind: any JobPostconditionVerifier]
    private let capacities: [WorkResourceClass: Int]
    private let ownerID: String
    private let leaseDuration: TimeInterval
    private let baseBackoff: TimeInterval
    private let clock: @Sendable () -> Date
    private var active: [UUID: (resource: WorkResourceClass, task: Task<Void, Never>)] = [:]
    private var wakeTask: Task<Void, Never>?
    private var idleWaiters: [CheckedContinuation<Void, Never>] = []
    private var started = false

    init(
        jobStore: JobStore,
        executors: [WorkJobKind: any JobExecutor],
        verifiers: [WorkJobKind: any JobPostconditionVerifier] = [:],
        capacities: [WorkResourceClass: Int] = [
            .planning: 1, .download: 3, .onDeviceSTT: 1, .remoteSTT: 2,
            .embedding: 2, .utilityLLM: 1, .scheduledAgent: 1, .notification: 2,
        ],
        ownerID: String = UUID().uuidString,
        leaseDuration: TimeInterval = 15 * 60,
        baseBackoff: TimeInterval = 30,
        clock: @escaping @Sendable () -> Date = Date.init
    ) {
        self.jobStore = jobStore
        self.executors = executors
        self.verifiers = verifiers
        self.capacities = capacities
        self.ownerID = ownerID
        self.leaseDuration = leaseDuration
        self.baseBackoff = max(0.01, baseBackoff)
        self.clock = clock
    }

    func start() {
        guard !started else { signal(); return }
        started = true
        do { try jobStore.reclaimExpiredLeases() }
        catch { Self.logger.error("Lease recovery failed: \(error, privacy: .public)") }
        signal()
    }

    func signal() {
        wakeTask?.cancel()
        wakeTask = nil
        pump()
    }

    func cancelActive() {
        started = false
        wakeTask?.cancel()
        wakeTask = nil
        for entry in active.values { entry.task.cancel() }
    }

    func stop() {
        started = false
        wakeTask?.cancel()
        wakeTask = nil
        cancelActive()
        resumeIdleWaiters()
    }

    func drainDueJobs() async {
        start()
        signal()
        if isDueWorkIdle { return }
        await withCheckedContinuation { idleWaiters.append($0) }
    }

    private func pump() {
        guard started else { return }
        if active.isEmpty {
            do { try jobStore.requeueAbandoned(owner: ownerID, now: clock()) }
            catch {
                Self.logger.error("Abandoned lease recovery failed: \(error, privacy: .public)")
            }
        }
        var launched = false
        for resource in WorkResourceClass.allCases {
            let capacity = max(0, capacities[resource] ?? 0)
            let occupied = active.values.filter { $0.resource == resource }.count
            let available = capacity - occupied
            guard available > 0 else { continue }
            do {
                let jobs = try jobStore.claimDueJobs(
                    resourceClass: resource,
                    capacity: capacity,
                    now: clock(),
                    owner: ownerID,
                    leaseDuration: leaseDuration
                )
                for job in jobs {
                    launched = true
                    launch(job)
                }
            } catch {
                Self.logger.error("Claim failed for \(resource.rawValue, privacy: .public): \(error, privacy: .public)")
            }
        }
        if !launched { scheduleNextWake() }
        if isDueWorkIdle { resumeIdleWaiters() }
    }

    private func launch(_ job: WorkJob) {
        guard let token = job.leaseToken else { return }
        let executor = executors[job.kind]
        let context = JobAttemptContext(job: job, leaseToken: token, deadline: job.leaseExpiresAt)
        let task = Task { [weak self] in
            let heartbeat = Task { [weak self] in
                await self?.renewLeaseUntilCancelled(jobID: job.id, leaseToken: token)
            }
            defer { heartbeat.cancel() }
            let result: Result<JobOutcome, Error>
            do {
                guard let executor else {
                    throw JobFailure(classification: .unexpected, message: "No executor for \(job.kind.rawValue)")
                }
                try self?.jobStore.markRunning(id: job.id, leaseToken: token)
                result = .success(try await executor.run(context))
            } catch {
                result = .failure(error)
            }
            await self?.finish(context, result: result)
        }
        active[job.id] = (job.resourceClass, task)
    }

    private func renewLeaseUntilCancelled(jobID: UUID, leaseToken: UUID) async {
        let interval = max(0.05, leaseDuration / 3)
        while !Task.isCancelled {
            do { try await Task.sleep(for: .seconds(interval)) }
            catch { return }
            guard !Task.isCancelled else { return }
            do {
                try jobStore.renewLease(
                    id: jobID,
                    leaseToken: leaseToken,
                    expiresAt: clock().addingTimeInterval(leaseDuration),
                    now: clock()
                )
            } catch { return }
        }
    }

    private func finish(_ context: JobAttemptContext, result: Result<JobOutcome, Error>) async {
        let job = context.job
        do {
            let outcome: JobOutcome
            switch result {
            case .success(let value): outcome = value
            case .failure(let failure as JobFailure): outcome = classify(failure, job: job)
            case .failure(let error):
                outcome = classify(
                    JobFailure(classification: error is CancellationError ? .cancelled : .unexpected,
                               message: error.localizedDescription),
                    job: job
                )
            }
            try await persist(outcome, context: context)
        } catch JobStoreError.transitionRejected {
            Self.logger.notice("Ignored stale completion for \(job.id, privacy: .public)")
        } catch {
            Self.logger.error("Job transition failed for \(job.id, privacy: .public): \(error, privacy: .public)")
            if context.deadline.map({ $0 > clock() }) ?? true {
                try? await Task.sleep(for: .milliseconds(50))
                await finish(context, result: result)
                return
            }
        }
        active[job.id] = nil
        pump()
    }

    private func persist(_ outcome: JobOutcome, context: JobAttemptContext) async throws {
        let job = context.job
        let token = context.leaseToken
        switch outcome {
        case .succeeded(let version):
            if let verifier = verifiers[job.kind] {
                if !(try await verifier.verifyAndCommit(job, leaseToken: token, outputVersion: version)) {
                    let failure = JobFailure(classification: .unexpected, message: "Required postcondition was not verified")
                    try jobStore.scheduleRetry(
                        id: job.id, leaseToken: token,
                        notBefore: backoffDate(for: job), error: failure
                    )
                }
            } else {
                try jobStore.complete(id: job.id, leaseToken: token, outputVersion: version)
            }
        case .retry(let date, let error):
            try jobStore.scheduleRetry(id: job.id, leaseToken: token, notBefore: date, error: error)
        case .blocked(let reason), .waitingForDependency(let reason):
            try jobStore.markBlocked(id: job.id, leaseToken: token, reason: reason)
        case .obsolete:
            try jobStore.markObsolete(id: job.id, leaseToken: token)
        case .cancelled:
            try jobStore.markCancelled(id: job.id, leaseToken: token)
        case .failedPermanent(let error):
            try jobStore.markFailedPermanent(id: job.id, leaseToken: token, error: error)
        }
    }

    private func classify(_ failure: JobFailure, job: WorkJob) -> JobOutcome {
        switch failure.classification {
        case .missingCredential, .unsafeToRetry: .blocked(reason: failure)
        case .missingDependency: .waitingForDependency(failure)
        case .invalidInput, .unsupportedFormat: .failedPermanent(failure)
        case .cancelled:
            .retry(notBefore: clock(), error: failure)
        case .transient, .rateLimited, .offline, .network, .corruptArtifact, .unexpected:
            job.attempt >= job.maxAttempts
                ? .failedPermanent(failure)
                : .retry(notBefore: backoffDate(for: job), error: failure)
        }
    }

    private func backoffDate(for job: WorkJob) -> Date {
        let exponent = min(max(0, job.attempt - 1), 8)
        return clock().addingTimeInterval(min(baseBackoff * pow(2, Double(exponent)), 60 * 60))
    }

    private var isDueWorkIdle: Bool {
        guard active.isEmpty else { return false }
        do {
            guard let date = try jobStore.nextDueDate() else { return true }
            return date > clock()
        } catch {
            return true
        }
    }

    private func scheduleNextWake() {
        wakeTask?.cancel()
        guard let date = try? jobStore.nextDueDate() else { return }
        wakeTask = Task { [weak self] in
            guard let self else { return }
            let delay = max(0.01, date.timeIntervalSince(self.clock()))
            try? await Task.sleep(for: .seconds(delay))
            guard !Task.isCancelled else { return }
            await self.signal()
        }
    }

    private func resumeIdleWaiters() {
        let waiters = idleWaiters
        idleWaiters.removeAll()
        waiters.forEach { $0.resume() }
    }
}
