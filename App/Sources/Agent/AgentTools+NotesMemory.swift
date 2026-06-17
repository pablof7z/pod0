import Foundation

extension AgentTools {
    private struct NotesMemoryPlan: Decodable {
        let error: String?
        let text: String?
        let content: String?
    }

    private struct NotesMemoryResult: Decodable {
        let success: Bool?
        let id: String?
        let summary: String?
        let activitySummary: String?
        let error: String?

        enum CodingKeys: String, CodingKey {
            case success, id, summary, error
            case activitySummary = "activity_summary"
        }
    }

    @MainActor
    static func dispatchNotesMemory(
        name: String,
        args: [String: Any],
        store: AppStateStore,
        batchID: UUID
    ) -> String {
        switch name {
        case Names.createNote:
            return createNote(args: args, store: store, batchID: batchID)
        case Names.recordMemory:
            return recordMemory(args: args, store: store, batchID: batchID)
        default:
            logger.warning("No template tool is registered for '\(name, privacy: .public)'")
            return toolError("Unknown tool: \(name)")
        }
    }

    @MainActor
    private static func createNote(args: [String: Any], store: AppStateStore, batchID: UUID) -> String {
        guard let plan = notesMemoryPlan(op: "note_plan", payload: args) else {
            return toolError("create_note planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        guard let text = plan.text else { return toolError("create_note plan was incomplete") }

        let kind: NoteKind = (args["kind"] as? String) == "reflection" ? .reflection : .free

        // Optional episode anchor — when the agent is responding to a voice
        // note or chapter context it should pass the episode_id so the note
        // surfaces in the chapter rail at the right position.
        let anchor: Anchor?
        if let episodeIDStr = args["episode_id"] as? String,
           let episodeID = UUID(uuidString: episodeIDStr) {
            let pos = (args["position_seconds"] as? Double) ?? 0
            anchor = .episode(id: episodeID, positionSeconds: pos)
        } else {
            anchor = nil
        }

        // Agent-authored: takes the no-publish branch in `addNote(...)` per
        // `identity-05-synthesis.md` §5.3.
        let note = store.addNote(text: text, kind: kind, target: anchor, author: .agent)
        let envelope = notesMemoryTool(
            op: "note_result",
            payload: ["id": note.id.uuidString, "text": text]
        )
        let result = decodeNotesMemoryResult(envelope)
        store.recordAgentActivity(
            AgentActivityEntry(
                batchID: batchID,
                kind: .noteCreated(noteID: note.id),
                summary: result?.activitySummary ?? "Saved note"
            )
        )
        return envelope ?? toolError("create_note result shaping is unavailable")
    }

    @MainActor
    private static func recordMemory(args: [String: Any], store: AppStateStore, batchID: UUID) -> String {
        guard let plan = notesMemoryPlan(op: "memory_plan", payload: args) else {
            return toolError("record_memory planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        guard let content = plan.content else { return toolError("record_memory plan was incomplete") }

        guard let response = store.kernel?.rememberTextMemory(content: content, source: "agent"),
              response.ok,
              let key = response.key ?? response.id else {
            return toolError("Could not save memory")
        }
        let envelope = notesMemoryTool(
            op: "memory_result",
            payload: [
                "id": key,
                "content": content,
                "message": response.message ?? "Saved memory",
            ]
        )
        let result = decodeNotesMemoryResult(envelope)
        store.recordAgentActivity(
            AgentActivityEntry(
                batchID: batchID,
                kind: .memoryFactRecorded(key: key),
                summary: result?.activitySummary ?? "Saved memory"
            )
        )
        return envelope ?? toolError("record_memory result shaping is unavailable")
    }

    @MainActor
    private static func notesMemoryPlan(op: String, payload: [String: Any]) -> NotesMemoryPlan? {
        guard let envelope = notesMemoryTool(op: op, payload: payload),
              let data = envelope.data(using: .utf8)
        else { return nil }
        return try? JSONDecoder().decode(NotesMemoryPlan.self, from: data)
    }

    private static func decodeNotesMemoryResult(_ envelope: String?) -> NotesMemoryResult? {
        guard let envelope,
              let data = envelope.data(using: .utf8)
        else { return nil }
        return try? JSONDecoder().decode(NotesMemoryResult.self, from: data)
    }

    @MainActor
    private static func notesMemoryTool(op: String, payload: [String: Any]) -> String? {
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
