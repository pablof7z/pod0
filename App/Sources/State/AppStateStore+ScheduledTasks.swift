import Foundation

// MARK: - Shared Agent Tasks

extension AppStateStore {

    var scheduledTasks: [AgentTaskSummary] {
        kernel?.podcastSnapshot?.agentTasks ?? []
    }

    @discardableResult
    func createScheduledPromptTask(title: String, prompt: String, schedule: String) -> DispatchResult {
        dispatchTaskAction([
            "op": "create_from_intent",
            "title": title,
            "intent": ["type": "agent_prompt", "prompt": prompt],
            "schedule": schedule,
        ])
    }

    @discardableResult
    func updateScheduledPromptTask(
        id: String,
        title: String,
        prompt: String,
        schedule: String
    ) -> DispatchResult {
        dispatchTaskAction([
            "op": "update_from_intent",
            "task_id": id,
            "title": title,
            "intent": ["type": "agent_prompt", "prompt": prompt],
            "schedule": schedule,
        ])
    }

    @discardableResult
    func removeScheduledTask(id: String) -> DispatchResult {
        dispatchTaskAction(["op": "delete", "task_id": id])
    }

    @discardableResult
    func setScheduledTaskEnabled(id: String, isEnabled: Bool) -> DispatchResult {
        dispatchTaskAction([
            "op": isEnabled ? "enable" : "disable",
            "task_id": id,
        ])
    }

    @discardableResult
    func runScheduledTaskNow(id: String) -> DispatchResult {
        dispatchTaskAction(["op": "run_now", "task_id": id])
    }

    private func dispatchTaskAction(_ body: [String: Any]) -> DispatchResult {
        guard let kernel else {
            return .failure("Rust kernel is not ready.")
        }
        return kernel.dispatch(namespace: "podcast.tasks", body: body)
    }
}
