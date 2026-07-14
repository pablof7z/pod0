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
        state.agentScheduledTasks.append(task)
        return task
    }

    func removeScheduledTask(id: UUID) {
        state.agentScheduledTasks.removeAll { $0.id == id }
    }

    func updateScheduledTask(id: UUID, label: String, prompt: String, intervalSeconds: TimeInterval) {
        guard let idx = state.agentScheduledTasks.firstIndex(where: { $0.id == id }) else { return }
        state.agentScheduledTasks[idx].label = label
        state.agentScheduledTasks[idx].prompt = prompt
        state.agentScheduledTasks[idx].intervalSeconds = intervalSeconds
        state.agentScheduledTasks[idx].nextRunAt = Date().addingTimeInterval(intervalSeconds)
    }

    /// Advances `nextRunAt` to `now + interval` — NOT `previousNextRunAt + interval`.
    /// This gives miss-once semantics: if the app was offline for N periods only
    /// one catch-up run fires; subsequent runs start fresh from the moment of resumption.
    func markTaskRun(id: UUID, now: Date = Date()) {
        guard let idx = state.agentScheduledTasks.firstIndex(where: { $0.id == id }) else { return }
        state.agentScheduledTasks[idx].lastRunAt = now
        state.agentScheduledTasks[idx].nextRunAt = now.addingTimeInterval(state.agentScheduledTasks[idx].intervalSeconds)
    }
}
