import Foundation

// MARK: - AgentSkillRegistry
//
// Central catalog of every `AgentSkill` known to the agent. Used by:
//   - `AgentPrompt` — to render the `## Skills` catalog
//   - `AgentChatSession+Turns` / `AgentRelayBridge` — to append per-turn
//     schemas for skills the session has activated
//   - `AgentTools.dispatchPodcast` — for the defensive ownership gate
//
// `all` and the simple lookups are nonisolated — they return `Sendable`
// value types. Only `schemas(for:)` requires `@MainActor` because invoking
// `AgentSkill.schema()` runs the @MainActor-isolated closure.

enum AgentSkillRegistry {

    /// All skills shipped with the app. Order is preserved when rendered into
    /// the system prompt catalog.
    static var all: [AgentSkill] {
        [
            PodcastGenerationSkill.skill,
            WikiResearchSkill.skill,
            ConversationHistorySkill.skill,
            YouTubeIngestionSkill.skill,
        ]
    }

    /// Looks up a skill by its canonical id.
    static func skill(id: String) -> AgentSkill? {
        all.first { $0.id == id }
    }

    /// Concatenated tool schemas for every skill in `enabledIDs`. Returns an
    /// empty array when nothing is enabled — keeps the per-turn tool list
    /// minimal by default.
    @MainActor
    static func schemas(for enabledIDs: Set<String>) -> [[String: Any]] {
        all
            .filter { enabledIDs.contains($0.id) }
            .flatMap { $0.schema() }
    }

    /// Reverse lookup: which skill (if any) owns the given tool name. Used by
    /// `dispatchPodcast` to gate skill-restricted tools defensively even
    /// though the LLM should never see them when the skill is off.
    static func owningSkillID(forTool name: String) -> String? {
        all.first { $0.toolNames.contains(name) }?.id
    }

    /// Every tool name owned by any skill in the registry. Used by tests and
    /// by the schema-coverage assertion in `AgentToolsPodcastTests`.
    static var allToolNames: Set<String> {
        Set(all.flatMap { $0.toolNames })
    }

    // MARK: - use_skill activation

    /// Result of a `use_skill` activation attempt.
    struct ActivationResult {
        /// JSON-encoded tool response to send back to the LLM as the
        /// `role: tool` message content.
        let resultJSON: String
        /// `enabledSkills` set after the activation. Same as input on error;
        /// otherwise input plus the activated skill id.
        let updatedEnabledSkills: Set<String>
    }

    /// Processes a `use_skill` tool call: parses the args JSON, validates
    /// against the registry, and returns the JSON-encoded tool response plus
    /// the updated `enabledSkills` set. Single source of truth used by both
    /// `AgentChatSession+Turns` and `AgentRelayBridge` so the activation
    /// contract is identical across the two entry points.
    static func activate(
        argsJSON: String,
        currentEnabledSkills enabled: Set<String>
    ) -> ActivationResult {
        let args = (try? JSONSerialization.jsonObject(with: Data(argsJSON.utf8))) as? [String: Any] ?? [:]
        let raw = args["skill_id"] as? String
        let skillID = raw?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        guard !skillID.isEmpty else {
            return ActivationResult(
                resultJSON: AgentTools.toolError("Missing or empty 'skill_id'"),
                updatedEnabledSkills: enabled
            )
        }
        guard let s = skill(id: skillID) else {
            let known = all.map(\.id).joined(separator: ", ")
            return ActivationResult(
                resultJSON: AgentTools.toolError("Unknown skill '\(skillID)'. Known skills: \(known)."),
                updatedEnabledSkills: enabled
            )
        }
        let alreadyEnabled = enabled.contains(s.id)
        var updated = enabled
        updated.insert(s.id)
        var payload: [String: Any] = [
            "skill_id": s.id,
            "display_name": s.displayName,
            "already_enabled": alreadyEnabled,
            "tools_unlocked": s.toolNames,
        ]
        // Skip re-sending the (large) manual when the skill was already
        // active — the LLM already received it earlier in the conversation.
        if !alreadyEnabled {
            payload["manual"] = s.manual
        }
        return ActivationResult(
            resultJSON: AgentTools.toolSuccess(payload),
            updatedEnabledSkills: updated
        )
    }
}
