import Foundation

/// Thin bridge for the system prompt injected at position 0 of every agent run.
/// Swift supplies raw facts already available in the render store; Rust owns
/// prompt prose, section ordering, caps, truncation, and fallback wording.
enum AgentPrompt {
    private struct PromptEnvelope: Decodable {
        let error: String?
        let systemPrompt: String?

        enum CodingKeys: String, CodingKey {
            case error
            case systemPrompt = "system_prompt"
        }
    }

    @MainActor
    static func build(
        for state: AppState,
        agentContext: AgentContextSnapshot?,
        memoryFacts: [MemoryFact]
    ) -> String {
        let request = requestPayload(for: state, agentContext: agentContext, memoryFacts: memoryFacts)
        guard let envelope = KernelModel.shared?.agentSystemPromptEnvelope(request: request),
              let data = envelope.data(using: .utf8),
              let decoded = try? JSONDecoder().decode(PromptEnvelope.self, from: data),
              decoded.error == nil,
              let prompt = decoded.systemPrompt,
              !prompt.isEmpty
        else {
            return ""
        }
        return prompt
    }

    @MainActor
    private static func requestPayload(
        for state: AppState,
        agentContext: AgentContextSnapshot?,
        memoryFacts: [MemoryFact]
    ) -> [String: Any] {
        var payload: [String: Any] = [
            "friends": state.friends.map {
                [
                    "display_name": $0.displayName,
                    "identifier": $0.identifier,
                ]
            },
            "notes": state.notes.map {
                [
                    "text": $0.text,
                    "kind": $0.kind.rawValue,
                    "deleted": $0.deleted,
                    "created_at": Int($0.createdAt.timeIntervalSince1970),
                ]
            },
            "memory_facts": memoryFacts.map {
                [
                    "key": $0.key,
                    "value": $0.value,
                ]
            },
            "skills": AgentSkillRegistry.all.map {
                [
                    "id": $0.id,
                    "summary": $0.summary,
                ]
            },
        ]
        if let agentContext {
            payload["agent_context"] = [
                "subscriptions": agentContext.subscriptions,
                "subscriptions_total": agentContext.subscriptionsTotal,
                "in_progress": agentContext.inProgress.map {
                    [
                        "title": $0.title,
                        "show_title": $0.showTitle,
                    ]
                },
                "recent_unplayed": agentContext.recentUnplayed.map {
                    [
                        "title": $0.title,
                        "show_title": $0.showTitle,
                    ]
                },
                "recent_window_days": agentContext.recentWindowDays,
            ]
        }
        return payload
    }
}
