import Foundation

// MARK: - JSON Schema (OpenAI tool format)

extension AgentTools {
    /// OpenAI-compatible tool schema array, ready to embed in a chat-completions request.
    @MainActor
    static var schema: [[String: Any]] {
        [
            tool(
                name: Names.createNote,
                description: "Save a note or reflection. When the user is listening to an episode, pass episode_id and position_seconds so the note appears anchored to that moment in the chapter rail.",
                properties: [
                    "text": ["type": "string", "description": "Note content"],
                    "kind": ["type": "string", "enum": ["free", "reflection"], "description": "Note type"],
                    "episode_id": ["type": "string", "description": "UUID of the episode to anchor this note to. Pass the currently-playing episode ID when available."],
                    "position_seconds": ["type": "number", "description": "Playhead position in seconds when the note is created. Anchors the note to that moment in the episode timeline."],
                ],
                required: ["text"]
            ),
            tool(
                name: Names.recordMemory,
                description: "Save something important to remember about the user for future sessions.",
                properties: ["content": ["type": "string", "description": "The fact to remember"]],
                required: ["content"]
            ),
            tool(
                name: Names.upgradeThinking,
                description: "Switch the agent to a stronger reasoning model for the rest of this conversation. Call this BEFORE answering when the task needs multi-step reasoning, planning, writing code, careful synthesis, or you are unsure. Do NOT call for simple lookups, single-tool answers, or short factual replies. Idempotent — calling again is harmless.",
                properties: [
                    "reason": [
                        "type": "string",
                        "description": "One short sentence explaining why the stronger model is needed.",
                    ],
                ],
                required: []
            ),
            tool(
                name: Names.useSkill,
                description: "Opt this conversation into a skill listed under '## Skills' in the system prompt. The tool result returns the skill's manual (focused instructions you should follow for the remainder of the conversation) and unlocks the skill's tools. Idempotent — re-calling is harmless. Call this BEFORE the first call to any skill-gated tool.",
                properties: [
                    "skill_id": [
                        "type": "string",
                        "description": "The skill's canonical id (e.g. 'podcast_generation'). See the '## Skills' section of the system prompt for available ids.",
                    ],
                ],
                required: ["skill_id"]
            ),
            tool(
                name: Names.ask,
                description: "Pause this turn to consult the owner via a modal sheet, then resume with their answer. Use sparingly — only when you genuinely need a decision (e.g. a peer agent over Nostr is requesting something sensitive and you want owner consent before acting). The tool result includes the owner's typed answer, or the sentinel strings 'user declined to answer' / 'user did not respond within 5 minutes'. Times out after 5 minutes. Do NOT call for casual clarifications — for those, just reply in chat and wait for the next turn.",
                properties: [
                    "question": [
                        "type": "string",
                        "description": "The question to ask the owner. Short, specific, action-oriented.",
                    ],
                    "context": [
                        "type": "string",
                        "description": "Optional one-or-two-line context shown under the question — e.g. who is asking, what tool would run next.",
                    ],
                ],
                required: ["question"]
            ),
            tool(
                name: Names.scheduleTask,
                description: "Schedule a recurring background task — the agent will automatically run the given prompt at the specified interval as a new headless conversation. Use this when the user asks for recurring background work (e.g. 'check Hacker News daily for interesting podcasts'). If the app is offline for multiple periods only one catch-up run fires on the next launch.",
                properties: [
                    "prompt": ["type": "string", "description": "The full task prompt to run on each scheduled execution."],
                    "label": ["type": "string", "description": "Short human-readable name shown in the scheduled tasks list (e.g. 'Daily HN check'). Defaults to the first 40 characters of the prompt."],
                    "interval_seconds": ["type": "integer", "description": "How often to run the task, in seconds. Omit if using 'cadence'."],
                    "cadence": ["type": "string", "enum": ["hourly", "daily", "weekly"], "description": "Convenience shorthand for common intervals. Omit if using 'interval_seconds'."],
                ],
                required: ["prompt"]
            ),
            tool(
                name: Names.cancelScheduledTask,
                description: "Cancel a previously scheduled recurring task by its ID. Use list_scheduled_tasks to find the task_id.",
                properties: [
                    "task_id": ["type": "string", "description": "The UUID of the scheduled task to cancel."],
                ],
                required: ["task_id"]
            ),
            tool(
                name: Names.listScheduledTasks,
                description: "List all currently active recurring scheduled tasks, including next run time and whether each is currently due.",
                properties: [:],
                required: []
            ),
        ]
    }

    /// Builds an OpenAI-compatible function-tool dictionary.
    private static func tool(
        name: String,
        description: String,
        properties: [String: Any],
        required: [String]
    ) -> [String: Any] {
        [
            "type": "function",
            "function": [
                "name": name,
                "description": description,
                "parameters": [
                    "type": "object",
                    "properties": properties,
                    "required": required,
                ] as [String: Any],
            ] as [String: Any],
        ]
    }
}
