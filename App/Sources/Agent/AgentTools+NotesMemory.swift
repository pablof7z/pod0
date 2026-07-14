import Foundation

extension AgentTools {

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
        guard let text = (args["text"] as? String)?.trimmed, !text.isEmpty else {
            return toolError("Missing note text")
        }

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
        store.recordAgentActivity(
            AgentActivityEntry(
                batchID: batchID,
                kind: .noteCreated(noteID: note.id),
                summary: "Saved note \"\(truncated(text))\""
            )
        )
        return toolSuccess(["id": note.id.uuidString, "summary": "Saved note"])
    }

    @MainActor
    private static func recordMemory(args: [String: Any], store: AppStateStore, batchID: UUID) -> String {
        guard let content = (args["content"] as? String)?.trimmed, !content.isEmpty else {
            return toolError("Missing memory content")
        }

        let memory = store.addAgentMemory(content: content)
        store.recordAgentActivity(
            AgentActivityEntry(
                batchID: batchID,
                kind: .memoryRecorded(memoryID: memory.id),
                summary: "Remembered \"\(truncated(content))\""
            )
        )
        return toolSuccess(["id": memory.id.uuidString, "summary": "Saved memory"])
    }
}
