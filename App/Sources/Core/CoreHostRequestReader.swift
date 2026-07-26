import Pod0Core

struct CoreHostRequestBatch: @unchecked Sendable {
    let cancellations: [HostCancellationRequest]
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
        let requests = requestCount > 0
            ? facade.nextHostRequests(maximumCount: requestCount)
            : []
        return CoreHostRequestBatch(
            cancellations: cancellations,
            requests: requests
        )
    }
}

/// Serializes transient observations away from rendering. Rust remains the
/// durable owner; this actor only changes where the typed call executes.
actor CoreTransientObservationRecorder {
    func record(_ observation: HostObservationEnvelope, in facade: Pod0Facade) {
        _ = facade.recordHostObservation(observation: observation)
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
                || (boundedCount > 0 && batch.requests.count == boundedCount) {
                executePendingRequests(from: facade, maximumCount: maximumCount)
            }
        }
    }
}
