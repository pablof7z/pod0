import Foundation

// MARK: - Schedule tool handlers

extension AgentTools {

    @MainActor
    static func dispatchSchedule(name: String, args: [String: Any], store: AppStateStore) -> String {
        switch name {
        case Names.scheduleTask:        return scheduleTaskTool(args: args, store: store)
        case Names.cancelScheduledTask: return cancelScheduledTaskTool(args: args, store: store)
        case Names.listScheduledTasks:  return listScheduledTasksTool(store: store)
        default:                        return toolError("Unknown schedule tool: \(name)")
        }
    }

    @MainActor
    private static func scheduleTaskTool(args: [String: Any], store: AppStateStore) -> String {
        guard let prompt = args["prompt"] as? String, !prompt.isEmpty else {
            return toolError("'prompt' is required")
        }
        let label = (args["label"] as? String) ?? String(prompt.prefix(40))

        let intervalSeconds: TimeInterval
        if let s = args["interval_seconds"] as? Double {
            intervalSeconds = s
        } else if let s = args["interval_seconds"] as? Int {
            intervalSeconds = TimeInterval(s)
        } else if let cadence = args["cadence"] as? String {
            switch cadence {
            case "hourly":  intervalSeconds = 3_600
            case "daily":   intervalSeconds = 86_400
            case "weekly":  intervalSeconds = 604_800
            default:
                return toolError("Unknown cadence '\(cadence)'. Use 'hourly', 'daily', or 'weekly', or provide 'interval_seconds'.")
            }
        } else {
            return toolError("Either 'interval_seconds' or 'cadence' is required.")
        }

        let task = store.addScheduledTask(label: label, prompt: prompt, intervalSeconds: intervalSeconds)
        return toolSuccess([
            "task_id": task.id.uuidString,
            "label": task.label,
            "interval_seconds": intervalSeconds,
            "next_run_at": iso8601Basic.string(from: task.nextRunAt),
        ])
    }

    @MainActor
    private static func cancelScheduledTaskTool(args: [String: Any], store: AppStateStore) -> String {
        guard let idString = args["task_id"] as? String,
              let id = UUID(uuidString: idString) else {
            return toolError("'task_id' must be a valid UUID string.")
        }
        guard store.state.agentScheduledTasks.contains(where: { $0.id == id }) else {
            return toolError("No scheduled task found with id '\(idString)'.")
        }
        store.removeScheduledTask(id: id)
        return toolSuccess()
    }

    @MainActor
    private static func listScheduledTasksTool(store: AppStateStore) -> String {
        let tasks = store.scheduledTasks
        let list: [[String: Any]] = tasks.map { task in
            var entry: [String: Any] = [
                "task_id": task.id.uuidString,
                "label": task.label,
                "prompt": task.prompt,
                "interval_seconds": task.intervalSeconds,
                "next_run_at": iso8601Basic.string(from: task.nextRunAt),
                "is_due": task.isDue,
            ]
            if let lastRun = task.lastRunAt {
                entry["last_run_at"] = iso8601Basic.string(from: lastRun)
            }
            return entry
        }
        return toolSuccess(["tasks": list, "count": tasks.count])
    }
}
