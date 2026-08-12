import Pod0Core

struct CoreHostRequestBatch: @unchecked Sendable {
    let cancellations: [HostCancellationRequest]
    let leasedRequests: [LeasedHostRequestEnvelope]
    let requests: [HostRequestEnvelope]
}

/// Keeps Rust-owned workflow reconciliation and SQLite reads off the main
/// actor. Calls are serialized so only one native drain can claim requests.
actor CoreHostRequestReader {
    func read(
        from facade: Pod0Facade,
        cancellationCount: UInt16,
        requestCount: UInt16
    ) -> CoreHostRequestBatch {
        let cancellations = facade.nextHostCancellations(
            maximumCount: cancellationCount
        )
        let leasedRequests = requestCount > 0
            ? facade.nextLeasedHostRequests(maximumCount: requestCount)
            : []
        let legacyCapacity = max(0, Int(requestCount) - leasedRequests.count)
        let requests = legacyCapacity > 0
            ? facade.nextHostRequests(maximumCount: UInt16(legacyCapacity))
            : []
        return CoreHostRequestBatch(
            cancellations: cancellations,
            leasedRequests: leasedRequests,
            requests: requests
        )
    }
}

extension Pod0NativeHostDispatcher {
    func executePendingRequests(from facade: Pod0Facade, maximumCount: UInt16 = 64) {
        guard executionEnabled else { return }
        guard observationRecoveryReady else {
            startObservationRecovery(from: facade, maximumCount: maximumCount)
            return
        }
        if retryRetainedObservations(in: facade) { return }
        if retryRetainedScheduledAgentObservations(in: facade) { return }
        requestDrainRequested = true
        guard requestDrainTask == nil else { return }
        let capacity = max(
            0,
            maximumConcurrentTasks - activeTasks.count - acknowledgementTasks.count
                - downloadRequests.count - scheduledAgentAcknowledgementTasks.count
                - pendingScheduledAgentExecutions.count
        )
        let boundedCount = min(Int(maximumCount), capacity)
        requestDrainRequested = false
        let reader = hostRequestReader
        requestDrainTask = Task { @MainActor [weak self] in
            let batch = await reader.read(
                from: facade,
                cancellationCount: maximumCount,
                requestCount: UInt16(boundedCount)
            )
            guard !Task.isCancelled, let self else { return }
            requestDrainTask = nil
            for cancellation in batch.cancellations {
                cancel(
                    requestID: cancellation.requestId,
                    cancellationID: cancellation.cancellationId
                )
            }
            for leased in batch.leasedRequests {
                execute(leased) { [weak self] observation in
                    guard let self else { return }
                    Task {
                        _ = await self.durableObservationRecorder.recordRetaining(
                            observation,
                            in: facade
                        )
                        await MainActor.run {
                            self.executePendingRequests(
                                from: facade,
                                maximumCount: maximumCount
                            )
                        }
                    }
                }
            }
            for envelope in batch.requests {
                execute(envelope) { [weak self] observation in
                    guard let self else { return }
                    record(observation, for: envelope, in: facade) { [weak self] in
                        self?.executePendingRequests(
                            from: facade,
                            maximumCount: maximumCount
                        )
                    }
                }
            }
            if requestDrainRequested
                || (boundedCount > 0
                    && batch.requests.count + batch.leasedRequests.count >= boundedCount) {
                executePendingRequests(from: facade, maximumCount: maximumCount)
            }
        }
    }
}
