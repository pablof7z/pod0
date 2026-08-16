import Foundation
import Pod0Core

extension SharedLibraryClient {
    func receiveScheduledAgents(
        _ projection: ScheduledAgentProjection,
        revision: UInt64
    ) {
        guard revision >= lastScheduledAgentRevision else { return }
        lastScheduledAgentRevision = revision
        let facade = facade
        scheduledAgentProjectionTask?.cancel()
        scheduledAgentProjectionTask = Task { @MainActor [weak self] in
            let snapshot = await Task.detached(priority: .utility) {
                Self.loadScheduledAgentPages(facade: facade, fallback: projection)
            }.value
            guard !Task.isCancelled,
                  let self,
                  revision == lastScheduledAgentRevision
            else { return }
            cachedScheduledAgent = snapshot
            if let store { publishScheduledAgents(to: store) }
        }
    }

    func publishScheduledAgents(to store: AppStateStore) {
        guard let projection = cachedScheduledAgent else { return }
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
    }

    @discardableResult
    func ensureScheduledTask(
        id: UUID,
        label: String,
        prompt: String,
        intervalSeconds: TimeInterval,
        modelReference: String,
        nextRunAt: Date
    ) async -> Bool {
        guard let interval = Self.intervalMilliseconds(intervalSeconds) else { return false }
        let taskID = ScheduledTaskId(uuid: id)
        return await executeScheduled(.ensureScheduledTask(task: ScheduledTaskInput(
            taskId: taskID,
            label: label,
            prompt: prompt,
            modelReference: modelReference,
            intervalMilliseconds: interval,
            nextRunAt: UnixTimestampMilliseconds(date: nextRunAt)
        )))
    }

    @discardableResult
    func updateScheduledTask(
        id: UUID,
        label: String,
        prompt: String,
        intervalSeconds: TimeInterval,
        modelReference: String,
        nextRunAt: Date
    ) async -> Bool {
        let taskID = ScheduledTaskId(uuid: id)
        let current = await scheduledProjection(taskID: taskID)
        guard let existing = Self.scheduledTask(in: current, id: taskID),
              let interval = Self.intervalMilliseconds(intervalSeconds) else { return false }
        return await executeScheduled(.updateScheduledTask(
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
    }

    @discardableResult
    func removeScheduledTask(id: UUID) async -> Bool {
        let taskID = ScheduledTaskId(uuid: id)
        let current = await scheduledProjection(taskID: taskID)
        guard let task = Self.scheduledTask(in: current, id: taskID) else { return false }
        return await executeScheduled(.removeScheduledTask(
            taskId: task.taskId,
            expectedTaskRevision: task.taskRevision
        ))
    }

    func reconcileScheduledAgents() {
        dispatchCoreCommand(.reconcileScheduledRuns)
    }

    func scheduledAgentWorkflow(taskID: UUID) -> ScheduledAgentWorkflowProjection? {
        cachedScheduledAgent?.workflows
            .filter { $0.taskId.uuid == taskID }
            .max { $0.updatedAt.value < $1.updatedAt.value }
    }

    func performScheduledAgentAction(
        _ action: WorkflowJobAction,
        taskID: UUID
    ) async -> WorkflowJobActionResult {
        guard let workflow = scheduledAgentWorkflow(taskID: taskID) else { return .notFound }
        switch action {
        case .retry where workflow.allowedActions.canRetry:
            let result = await executeWorkflowAction(.retryScheduledRun(
                occurrenceId: workflow.occurrenceId,
                expectedWorkflowRevision: workflow.workflowRevision
            ), action: action)
            if case .accepted = result {
                dispatchCoreCommand(.reconcileScheduledRuns)
            }
            return result
        case .cancel where workflow.allowedActions.canCancel:
            return await executeWorkflowAction(.cancelScheduledRun(
                occurrenceId: workflow.occurrenceId,
                expectedWorkflowRevision: workflow.workflowRevision
            ), action: action)
        default:
            return .notAllowed
        }
    }
}

extension SharedLibraryClient {
    nonisolated static func loadScheduledAgentPages(
        facade: Pod0Facade,
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

    private func scheduledProjection(
        taskID: ScheduledTaskId
    ) async -> ProjectionEnvelope {
        await coreSnapshot(ProjectionRequest(
            scope: .scheduledAgent(taskId: taskID),
            offset: 0,
            maxItems: 1
        ))
    }

    private func executeScheduled(_ command: ApplicationCommand) async -> Bool {
        do {
            _ = try await execute(command)
            return true
        } catch {
            return false
        }
    }

    nonisolated private static func scheduledTask(
        in envelope: ProjectionEnvelope?,
        id: ScheduledTaskId
    ) -> ScheduledTaskProjection? {
        guard let envelope,
              case .scheduledAgent(let projection) = envelope.projection,
              projection.failure == nil
        else { return nil }
        return projection.tasks.first { $0.taskId == id }
    }

    static func intervalMilliseconds(_ seconds: TimeInterval) -> UInt64? {
        guard seconds.isFinite, seconds > 0,
              seconds <= Double(UInt64.max) / 1_000 else { return nil }
        return UInt64((seconds * 1_000).rounded())
    }
}
