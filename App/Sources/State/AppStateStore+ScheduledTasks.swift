import Foundation

// MARK: - Rust-owned scheduled tasks

extension AppStateStore {
    var scheduledTasks: [AgentScheduledTask] { state.agentScheduledTasks }

    /// Replaces the native read model from the typed Rust projection.
    /// Persistence strips this cache once shared scheduled authority is active.
    func applySharedScheduledTasks(_ tasks: [AgentScheduledTask]) {
        guard state.agentScheduledTasks != tasks else { return }
        mutateProjectionState { $0.agentScheduledTasks = tasks }
    }

    @discardableResult
    func addScheduledTask(
        label: String,
        prompt: String,
        intervalSeconds: TimeInterval
    ) -> AgentScheduledTask {
        let now = Date()
        let task = AgentScheduledTask(
            id: UUID(),
            label: label,
            prompt: prompt,
            intervalSeconds: intervalSeconds,
            createdAt: now,
            lastRunAt: nil,
            nextRunAt: now.addingTimeInterval(intervalSeconds)
        )
        let sharedLibrary = sharedLibrary
        let modelReference = state.settings.agentInitialModel
        Task {
            _ = await sharedLibrary?.ensureScheduledTask(
                id: task.id,
                label: label,
                prompt: prompt,
                intervalSeconds: intervalSeconds,
                modelReference: modelReference,
                nextRunAt: task.nextRunAt
            )
        }
        return scheduledTasks.first(where: { $0.id == task.id }) ?? task
    }

    func removeScheduledTask(id: UUID) {
        let sharedLibrary = sharedLibrary
        Task { _ = await sharedLibrary?.removeScheduledTask(id: id) }
    }

    func updateScheduledTask(
        id: UUID,
        label: String,
        prompt: String,
        intervalSeconds: TimeInterval
    ) {
        let sharedLibrary = sharedLibrary
        let modelReference = state.settings.agentInitialModel
        let nextRunAt = Date().addingTimeInterval(intervalSeconds)
        Task {
            _ = await sharedLibrary?.updateScheduledTask(
                id: id,
                label: label,
                prompt: prompt,
                intervalSeconds: intervalSeconds,
                modelReference: modelReference,
                nextRunAt: nextRunAt
            )
        }
    }
}
