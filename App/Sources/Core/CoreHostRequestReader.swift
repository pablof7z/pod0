import Pod0Core

struct CoreHostRequestBatch: @unchecked Sendable {
    let leasedRequests: [LeasedHostRequestEnvelope]
}

/// Keeps Rust-owned workflow reconciliation and SQLite reads off the main
/// actor. Calls are serialized so only one native drain can claim requests.
actor CoreHostRequestReader {
    func read(
        from facade: Pod0Facade,
        requestCount: UInt16
    ) -> CoreHostRequestBatch {
        let leasedRequests = requestCount > 0
            ? facade.nextLeasedHostRequests(maximumCount: requestCount)
            : []
        return CoreHostRequestBatch(
            leasedRequests: leasedRequests
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
        requestDrainRequested = true
        guard requestDrainTask == nil else { return }
        let capacity = max(
            0,
            maximumConcurrentTasks - activeTasks.count - downloadRequests.count
                - pendingScheduledAgentExecutions.count
        )
        let boundedCount = min(Int(maximumCount), capacity)
        requestDrainRequested = false
        let reader = hostRequestReader
        requestDrainTask = Task { @MainActor [weak self] in
            let batch = await reader.read(
                from: facade,
                requestCount: UInt16(boundedCount)
            )
            guard !Task.isCancelled, let self else { return }
            requestDrainTask = nil
            for leased in batch.leasedRequests {
                execute(leased) { [weak self] observation in
                    guard let self else { return }
                    Task {
                        let receipt = await self.durableObservationRecorder.recordRetaining(
                            observation,
                            in: facade
                        )
                        await MainActor.run {
                            self.handleLeasedObservation(
                                observation,
                                receipt: receipt
                            )
                            self.executePendingRequests(
                                from: facade,
                                maximumCount: maximumCount
                            )
                        }
                    }
                }
            }
            if requestDrainRequested
                || (boundedCount > 0
                    && batch.leasedRequests.count >= boundedCount) {
                executePendingRequests(from: facade, maximumCount: maximumCount)
            }
        }
    }

    private func handleLeasedObservation(
        _ leased: LeasedHostObservationEnvelope,
        receipt: HostObservationReceipt
    ) {
        let requestID = leased.observation.requestId
        if leased.observation.observation.isDownloadResult {
            downloadHost.retire(
                requestID: requestID,
                observation: leased.observation.observation,
                receipt: receipt
            )
            if Self.downloadReceiptAllowsRetirement(receipt) {
                downloadRequests[requestID] = nil
                rememberCompletion(requestID)
            }
        }
        if case .persisted(_, let terminal) = receipt,
           !terminal,
           case .scheduledAgentExecutionObserved(.accepted) = leased.observation.observation {
            beginPersistedScheduledAgentExecution(for: requestID)
        }
        if Self.receiptAllowsRetirement(receipt) {
            rememberCompletion(requestID)
        }
    }

    private static func receiptAllowsRetirement(_ receipt: HostObservationReceipt) -> Bool {
        switch receipt {
        case .persisted(_, let terminal): terminal
        case .rejected: true
        case .acceptedTransient, .retainAndRetry: false
        }
    }

    private static func downloadReceiptAllowsRetirement(
        _ receipt: HostObservationReceipt
    ) -> Bool {
        receiptAllowsRetirement(receipt)
    }
}
