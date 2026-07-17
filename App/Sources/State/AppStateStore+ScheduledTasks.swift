import Foundation

// MARK: - Scheduled Tasks

extension AppStateStore {

    var scheduledTasks: [AgentScheduledTask] { state.agentScheduledTasks }

    @discardableResult
    func addScheduledTask(label: String, prompt: String, intervalSeconds: TimeInterval) -> AgentScheduledTask {
        let task = AgentScheduledTask(
            id: UUID(),
            label: label,
            prompt: prompt,
            intervalSeconds: intervalSeconds,
            createdAt: Date(),
            lastRunAt: nil,
            nextRunAt: Date().addingTimeInterval(intervalSeconds)
        )
        mutateState { $0.agentScheduledTasks.append(task) }
        return task
    }

    func removeScheduledTask(id: UUID) {
        mutateState { $0.agentScheduledTasks.removeAll { $0.id == id } }
    }

    func updateScheduledTask(id: UUID, label: String, prompt: String, intervalSeconds: TimeInterval) {
        guard let idx = state.agentScheduledTasks.firstIndex(where: { $0.id == id }) else { return }
        mutateState {
            $0.agentScheduledTasks[idx].label = label
            $0.agentScheduledTasks[idx].prompt = prompt
            $0.agentScheduledTasks[idx].intervalSeconds = intervalSeconds
            $0.agentScheduledTasks[idx].nextRunAt = Date().addingTimeInterval(intervalSeconds)
        }
    }

    /// Advances `nextRunAt` to `now + interval` — NOT `previousNextRunAt + interval`.
    /// This gives miss-once semantics: if the app was offline for N periods only
    /// one catch-up run fires; subsequent runs start fresh from the moment of resumption.
    func markTaskRun(id: UUID, now: Date = Date()) {
        guard let idx = state.agentScheduledTasks.firstIndex(where: { $0.id == id }) else { return }
        let interval = state.agentScheduledTasks[idx].intervalSeconds
        mutateState {
            $0.agentScheduledTasks[idx].lastRunAt = now
            $0.agentScheduledTasks[idx].nextRunAt = now.addingTimeInterval(interval)
        }
    }

    /// Repairs the small success-marker/schedule-projection gap. Only the
    /// exact due occurrence may advance a recurring definition, and a second
    /// pass is a no-op because `nextRunAt` has moved to a new identity.
    @discardableResult
    func advanceCompletedScheduledOccurrences(
        from jobs: [WorkJob],
        now: Date = Date()
    ) -> Int {
        let succeeded = Set(
            jobs.lazy.filter { $0.kind == .scheduledAgentRun && $0.state == .succeeded }
                .map(\.idempotencyKey)
        )
        var tasks = state.agentScheduledTasks
        var advanced = 0
        for index in tasks.indices where tasks[index].nextRunAt <= now {
            let key = DesiredStatePlanner.scheduledOccurrenceID(
                taskID: tasks[index].id,
                scheduledFor: tasks[index].nextRunAt
            )
            guard succeeded.contains(key) else { continue }
            tasks[index].lastRunAt = now
            tasks[index].nextRunAt = now.addingTimeInterval(tasks[index].intervalSeconds)
            advanced += 1
        }
        if advanced > 0 { mutateState { $0.agentScheduledTasks = tasks } }
        return advanced
    }
}
