import Foundation
import Pod0Core

extension SharedLibraryClient {
    func subscribeToScheduledAgents(_ subscriber: SharedLibrarySubscriber) {
        scheduledAgentSubscriptionID = facade.subscribe(
            request: ProjectionRequest(
                scope: .scheduledAgent(taskId: nil),
                offset: 0,
                maxItems: 200
            ),
            subscriber: subscriber
        )
    }

    func unsubscribeFromScheduledAgents() {
        if let scheduledAgentSubscriptionID {
            facade.unsubscribe(subscriptionId: scheduledAgentSubscriptionID)
        }
        scheduledAgentSubscriptionID = nil
        cachedScheduledAgent = nil
        workflowClient?.detachScheduledAgentCore()
    }

    func receiveScheduledAgents(
        _ projection: ScheduledAgentProjection,
        revision: UInt64
    ) {
        guard revision >= lastScheduledAgentRevision else { return }
        lastScheduledAgentRevision = revision
        cachedScheduledAgent = loadScheduledAgentPages(fallback: projection)
        if let store { publishScheduledAgents(to: store) }
        dispatcher.executePendingRequests(from: facade)
    }

    func publishScheduledAgents(to store: AppStateStore) {
        let projection = cachedScheduledAgent ?? loadScheduledAgentPages(fallback: nil)
        cachedScheduledAgent = projection
        store.applySharedScheduledTasks(projection.tasks.compactMap { task in
            guard let id = task.taskId.uuid else { return nil }
            return AgentScheduledTask(
                id: id,
                label: task.label,
                prompt: task.prompt,
                intervalSeconds: Double(task.intervalMilliseconds) / 1_000,
                createdAt: task.createdAt.date,
                lastRunAt: task.lastRunAt?.date,
                nextRunAt: task.nextRunAt.date
            )
        })
        workflowClient?.attachScheduledAgentCore(projection.workflows)
    }

    @discardableResult
    func ensureScheduledTask(
        id: UUID,
        label: String,
        prompt: String,
        intervalSeconds: TimeInterval,
        modelReference: String,
        nextRunAt: Date
    ) -> Bool {
        guard let interval = Self.intervalMilliseconds(intervalSeconds) else { return false }
        dispatchScheduled(.ensureScheduledTask(task: ScheduledTaskInput(
            taskId: ScheduledTaskId(uuid: id),
            label: label,
            prompt: prompt,
            modelReference: modelReference,
            intervalMilliseconds: interval,
            nextRunAt: UnixTimestampMilliseconds(date: nextRunAt)
        )))
        return scheduledTask(id: id) != nil
    }

    @discardableResult
    func updateScheduledTask(
        id: UUID,
        label: String,
        prompt: String,
        intervalSeconds: TimeInterval,
        modelReference: String,
        nextRunAt: Date
    ) -> Bool {
        guard let existing = scheduledTask(id: id),
              let interval = Self.intervalMilliseconds(intervalSeconds) else { return false }
        dispatchScheduled(.updateScheduledTask(
            taskId: existing.taskId,
            expectedTaskRevision: existing.taskRevision,
            task: ScheduledTaskInput(
                taskId: existing.taskId,
                label: label,
                prompt: prompt,
                modelReference: modelReference,
                intervalMilliseconds: interval,
                nextRunAt: UnixTimestampMilliseconds(date: nextRunAt)
            )
        ))
        guard let updated = scheduledTask(id: id) else { return false }
        return updated.taskRevision.value > existing.taskRevision.value
    }

    @discardableResult
    func removeScheduledTask(id: UUID) -> Bool {
        guard let task = scheduledTask(id: id) else { return false }
        dispatchScheduled(.removeScheduledTask(
            taskId: task.taskId,
            expectedTaskRevision: task.taskRevision
        ))
        return scheduledTask(id: id) == nil
    }

    func reconcileScheduledAgents() {
        dispatchScheduled(.reconcileScheduledRuns)
    }

    func performScheduledAgentAction(
        _ action: WorkflowJobAction,
        on projection: WorkflowJobProjection
    ) -> WorkflowJobActionResult {
        guard projection.authority == .sharedRustScheduledAgents,
              projection.allowedActions.contains(action),
              let workflow = cachedScheduledAgent?.workflows.first(where: {
                  WorkflowJobProjection(scheduledAgentWorkflow: $0).id == projection.id
              }),
              workflow.workflowRevision.value == projection.coreWorkflowRevision
        else { return .stale }
        switch action {
        case .retry:
            dispatchScheduled(.retryScheduledRun(
                occurrenceId: workflow.occurrenceId,
                expectedWorkflowRevision: workflow.workflowRevision
            ))
            dispatchScheduled(.reconcileScheduledRuns)
        case .cancel:
            dispatchScheduled(.cancelScheduledRun(
                occurrenceId: workflow.occurrenceId,
                expectedWorkflowRevision: workflow.workflowRevision
            ))
        }
        return .accepted(action)
    }
}

private extension SharedLibraryClient {
    func loadScheduledAgentPages(
        fallback: ScheduledAgentProjection?
    ) -> ScheduledAgentProjection {
        var offset: UInt32 = 0
        var tasks: [ScheduledTaskId: ScheduledTaskProjection] = [:]
        var workflows: [ScheduledOccurrenceId: ScheduledAgentWorkflowProjection] = [:]
        var failure: CoreFailure?
        while true {
            let envelope = facade.snapshot(request: ProjectionRequest(
                scope: .scheduledAgent(taskId: nil),
                offset: offset,
                maxItems: 200
            ))
            guard case .scheduledAgent(let page) = envelope.projection else { break }
            for task in page.tasks { tasks[task.taskId] = task }
            for workflow in page.workflows { workflows[workflow.occurrenceId] = workflow }
            failure = failure ?? page.failure
            guard page.hasMore, offset <= UInt32.max - 200 else { break }
            offset += 200
        }
        if tasks.isEmpty, workflows.isEmpty, let fallback {
            return fallback
        }
        return ScheduledAgentProjection(
            tasks: tasks.values.sorted { lhs, rhs in
                lhs.taskId.high == rhs.taskId.high
                    ? lhs.taskId.low < rhs.taskId.low
                    : lhs.taskId.high < rhs.taskId.high
            },
            workflows: workflows.values.sorted {
                $0.updatedAt.value > $1.updatedAt.value
            },
            hasMore: false,
            failure: failure
        )
    }

    func scheduledTask(id: UUID) -> ScheduledTaskProjection? {
        let taskID = ScheduledTaskId(uuid: id)
        return loadScheduledAgentPages(fallback: nil).tasks.first { $0.taskId == taskID }
    }

    func dispatchScheduled(_ command: ApplicationCommand) {
        facade.dispatch(command: CommandEnvelope(
            commandId: CommandId(uuid: UUID()),
            cancellationId: CancellationId(uuid: UUID()),
            expectedRevision: nil,
            command: command
        ))
        dispatcher.executePendingRequests(from: facade)
    }

    static func intervalMilliseconds(_ seconds: TimeInterval) -> UInt64? {
        guard seconds.isFinite, seconds > 0,
              seconds <= Double(UInt64.max) / 1_000 else { return nil }
        return UInt64((seconds * 1_000).rounded())
    }
}
