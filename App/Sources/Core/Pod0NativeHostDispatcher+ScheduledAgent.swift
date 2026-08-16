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

}
