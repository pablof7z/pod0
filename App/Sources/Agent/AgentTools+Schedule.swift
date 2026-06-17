import Foundation

// MARK: - Schedule tool handlers

extension AgentTools {
    private struct SchedulePlan: Decodable {
        let error: String?
        let title: String?
        let prompt: String?
        let schedule: String?
    }

    private struct CancelSchedulePlan: Decodable {
        let error: String?
        let taskID: String?

        enum CodingKeys: String, CodingKey {
            case error
            case taskID = "task_id"
        }
    }

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
        guard let plan = schedulePlan(op: "schedule_plan", payload: args, as: SchedulePlan.self) else {
            return toolError("schedule_task planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        guard let title = plan.title,
              let prompt = plan.prompt,
              let schedule = plan.schedule
        else { return toolError("schedule_task plan was incomplete") }
        switch store.createScheduledPromptTask(title: title, prompt: prompt, schedule: schedule) {
        case .accepted:
            return scheduleTool(op: "schedule_result", payload: [
                "title": title,
                "schedule": schedule,
                "prompt": prompt,
            ]) ?? toolError("schedule_task result shaping is unavailable")
        case .failure(let message):
            return toolError(message)
        }
    }

    @MainActor
    private static func cancelScheduledTaskTool(args: [String: Any], store: AppStateStore) -> String {
        guard let plan = schedulePlan(op: "cancel_schedule_plan", payload: args, as: CancelSchedulePlan.self) else {
            return toolError("cancel_scheduled_task planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        guard let id = plan.taskID else { return toolError("cancel_scheduled_task plan was incomplete") }
        switch store.removeScheduledTask(id: id) {
        case .accepted:
            return scheduleTool(op: "cancel_schedule_result", payload: ["task_id": id])
                ?? toolError("cancel_scheduled_task result shaping is unavailable")
        case .failure(let message):
            return toolError(message)
        }
    }

    @MainActor
    private static func listScheduledTasksTool(store: AppStateStore) -> String {
        let tasks = store.scheduledTasks
        let list: [[String: Any]] = tasks.map { task in
            var entry: [String: Any] = [
                "task_id": task.id,
                "title": task.title,
                "schedule": task.schedule,
                "status": task.status,
                "enabled": task.isEnabled,
                "intent_type": task.intentType ?? "custom",
                "intent_label": task.intentLabel ?? "Custom task",
            ]
            if let detail = task.intentDetail {
                entry["intent_detail"] = detail
            }
            if let description = task.description {
                entry["description"] = description
            }
            if let nextRunAt = task.nextRunAt {
                entry["next_run_at"] = Int(nextRunAt.timeIntervalSince1970)
            }
            if let lastRunAt = task.lastRunAt {
                entry["last_run_at"] = Int(lastRunAt.timeIntervalSince1970)
            }
            return entry
        }
        return scheduleTool(op: "schedule_list_result", payload: ["tasks": list])
            ?? toolError("list_scheduled_tasks result shaping is unavailable")
    }

    @MainActor
    private static func schedulePlan<T: Decodable>(
        op: String,
        payload: [String: Any],
        as type: T.Type
    ) -> T? {
        guard let envelope = scheduleTool(op: op, payload: payload),
              let data = envelope.data(using: .utf8)
        else { return nil }
        return try? JSONDecoder().decode(T.self, from: data)
    }

    @MainActor
    private static func scheduleTool(op: String, payload: [String: Any]) -> String? {
        guard let handle = KernelModel.shared?.podcastHandlePointer else { return nil }
        var request = payload
        request["op"] = op
        guard let data = try? JSONSerialization.data(withJSONObject: request),
              let json = String(data: data, encoding: .utf8)
        else { return nil }
        return json.withCString { ptr in
            guard let result = nmp_app_podcast_agent_action_tool(handle, ptr) else {
                return nil
            }
            defer { nmp_free_string(result) }
            return String(cString: result)
        }
    }
}
