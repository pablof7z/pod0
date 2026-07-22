import Pod0Core

extension Pod0NativeHostDispatcher {
    func startScheduledAgentTask(
        _ envelope: HostRequestEnvelope,
        execution: ScheduledAgentExecutionRequest,
        delivery: @escaping Delivery
    ) {
        pendingScheduledAgentExecutions[envelope.requestId] = PendingScheduledAgentExecution(
            envelope: envelope,
            execution: execution,
            delivery: delivery
        )
        delivery(makeEnvelope(
            envelope,
            sequenceNumber: 0,
            observedAt: now(),
            observation: .scheduledAgentExecutionObserved(observation: .accepted(
                occurrenceId: execution.occurrenceId,
                attemptId: execution.attemptId,
                providerOperationId: nil
            ))
        ))
    }

    func beginPersistedScheduledAgentExecution(for requestID: HostRequestId) {
        guard let pending = pendingScheduledAgentExecutions.removeValue(forKey: requestID),
              activeTasks[requestID] == nil
        else { return }
        let task = Task { @MainActor [weak self] in
            guard let self else { return }
            let result = await scheduledAgentHost.execute(pending.execution)
            guard activeTasks.removeValue(forKey: pending.envelope.requestId) != nil else { return }
            let final = isExpired(pending.envelope)
                ? expiredScheduledAgentObservation(pending.execution)
                : result
            finish(
                pending.envelope,
                sequenceNumber: 1,
                observation: .scheduledAgentExecutionObserved(observation: final),
                delivery: pending.delivery,
                remember: false
            )
        }
        activeTasks[pending.envelope.requestId] = ActiveTask(
            envelope: pending.envelope,
            task: task,
            delivery: pending.delivery
        )
    }

    func enqueueScheduledAgentObservation(
        _ observation: HostObservationEnvelope,
        for envelope: HostRequestEnvelope,
        in facade: Pod0Facade,
        completion: @escaping @MainActor () -> Void
    ) {
        pendingScheduledAgentObservations[envelope.requestId, default: []].append(observation)
        scheduledAgentObservationCompletions[envelope.requestId] = completion
        recordNextScheduledAgentObservation(for: envelope.requestId, in: facade)
    }

    @discardableResult
    func retryRetainedScheduledAgentObservations(in facade: Pod0Facade) -> Bool {
        let requestIDs = retainedScheduledAgentObservationIDs.filter {
            scheduledAgentAcknowledgementTasks[$0] == nil
        }
        for requestID in requestIDs {
            recordNextScheduledAgentObservation(for: requestID, in: facade)
        }
        return !requestIDs.isEmpty
    }

    private func recordNextScheduledAgentObservation(
        for requestID: HostRequestId,
        in facade: Pod0Facade
    ) {
        guard scheduledAgentAcknowledgementTasks[requestID] == nil,
              let observation = pendingScheduledAgentObservations[requestID]?.first
        else { return }
        let recorder = durableObservationRecorder
        scheduledAgentAcknowledgementTasks[requestID] = Task { @MainActor [weak self] in
            let receipt = await recorder.recordRetaining(
                observation,
                in: facade,
                persistForRelaunch: true
            )
            guard let self else { return }
            scheduledAgentAcknowledgementTasks.removeValue(forKey: requestID)
            if case .retainAndRetry = receipt {
                retainedScheduledAgentObservationIDs.insert(requestID)
                return
            }
            retainedScheduledAgentObservationIDs.remove(requestID)
            let recorded = pendingScheduledAgentObservations[requestID]?.first
            pendingScheduledAgentObservations[requestID]?.removeFirst()
            if Self.scheduledAgentReceiptAllowsRetirement(receipt) {
                retireScheduledAgentObservationQueue(requestID)
                return
            }
            if case .persisted(_, let terminal) = receipt,
               !terminal,
               let recorded,
               case .scheduledAgentExecutionObserved(.accepted) = recorded.observation {
                beginPersistedScheduledAgentExecution(for: requestID)
            }
            if pendingScheduledAgentObservations[requestID]?.isEmpty == false {
                recordNextScheduledAgentObservation(for: requestID, in: facade)
            } else {
                finishScheduledAgentObservationQueue(requestID)
            }
        }
    }

    private func retireScheduledAgentObservationQueue(_ requestID: HostRequestId) {
        pendingScheduledAgentObservations[requestID] = nil
        pendingScheduledAgentExecutions[requestID] = nil
        retainedScheduledAgentObservationIDs.remove(requestID)
        rememberCompletion(requestID)
        finishScheduledAgentObservationQueue(requestID)
    }

    private func finishScheduledAgentObservationQueue(_ requestID: HostRequestId) {
        pendingScheduledAgentObservations[requestID] = nil
        let completion = scheduledAgentObservationCompletions.removeValue(forKey: requestID)
        completion?()
    }

    func cancelScheduledAgentTask(_ active: ActiveTask) -> Bool {
        guard case .executeScheduledAgentTurn(let execution) = active.envelope.request else {
            return false
        }
        active.task.cancel()
        finish(
            active.envelope,
            sequenceNumber: 1,
            observation: .scheduledAgentExecutionObserved(observation: .cancelled(
                occurrenceId: execution.occurrenceId,
                attemptId: execution.attemptId
            )),
            delivery: active.delivery,
            remember: false
        )
        return true
    }

    func cancelPendingScheduledAgentExecution(
        requestID: HostRequestId,
        cancellationID: CancellationId
    ) -> Bool {
        guard let pending = pendingScheduledAgentExecutions[requestID],
              pending.envelope.cancellationId == cancellationID
        else { return false }
        pendingScheduledAgentExecutions[requestID] = nil
        finish(
            pending.envelope,
            sequenceNumber: 1,
            observation: .scheduledAgentExecutionObserved(observation: .cancelled(
                occurrenceId: pending.execution.occurrenceId,
                attemptId: pending.execution.attemptId
            )),
            delivery: pending.delivery,
            remember: false
        )
        return true
    }

    func expiredScheduledAgentObservation(
        _ execution: ScheduledAgentExecutionRequest
    ) -> ScheduledAgentExecutionObservation {
        .failed(
            occurrenceId: execution.occurrenceId,
            attemptId: execution.attemptId,
            code: .network,
            safeDetail: "Scheduled provider deadline expired",
            retryAfterMilliseconds: nil
        )
    }

    private static func scheduledAgentReceiptAllowsRetirement(
        _ receipt: HostObservationReceipt
    ) -> Bool {
        switch receipt {
        case .persisted(_, let terminal): terminal
        case .rejected: true
        case .acceptedTransient, .retainAndRetry: false
        }
    }
}
