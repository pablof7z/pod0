import Foundation

/// Generates a short title for a chat conversation via the user-configured
/// Rust memory-compilation model. Designed to run as a fire-and-forget
/// background task after the first assistant text reply lands.
///
/// Rust owns message selection, transcript truncation, prompt text, title
/// constraints, and response parsing. Swift only executes the provider call.
enum AgentChatTitleGenerator {

    struct Plan: Sendable {
        let model: String
        let systemPrompt: String
        let userPrompt: String
    }

    private struct RustPromptResponse: Decodable {
        let error: String?
        let model: String?
        let systemPrompt: String?
        let userPrompt: String?
    }

    private struct RustParseResponse: Decodable {
        let error: String?
        let title: String?
    }

    private static let decoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return decoder
    }()

    static func makePlan(from messages: [ChatMessage], store: AppStateStore) -> Plan? {
        guard let envelope = store.kernel?.agentChatTitlePromptEnvelope(
            messages: messages.map { ["role": roleName($0.role), "text": $0.text] }
        ),
              let data = envelope.data(using: .utf8),
              let decoded = try? decoder.decode(RustPromptResponse.self, from: data),
              decoded.error == nil,
              let model = decoded.model,
              let systemPrompt = decoded.systemPrompt,
              let userPrompt = decoded.userPrompt,
              !model.isEmpty,
              !systemPrompt.isEmpty,
              !userPrompt.isEmpty
        else {
            return nil
        }
        return Plan(model: model, systemPrompt: systemPrompt, userPrompt: userPrompt)
    }

    /// Returns the generated title, or `nil` if generation failed (missing
    /// credential, network error, unparseable response). Callers should treat
    /// a nil return as "leave the conversation untitled and try again later".
    static func generate(plan: Plan) async -> String? {
        let client = ProviderCompletionClient.live(model: plan.model)
        do {
            let json = try await client.compile(
                systemPrompt: plan.systemPrompt,
                userPrompt: plan.userPrompt,
                feature: CostFeature.agentChatTitle
            )
            return await parseTitle(rawContent: json)
        } catch {
            return nil
        }
    }

    @MainActor
    private static func parseTitle(rawContent: String) -> String? {
        guard let envelope = KernelModel.shared?.agentChatTitleParseEnvelope(rawContent: rawContent),
              let data = envelope.data(using: .utf8),
              let decoded = try? decoder.decode(RustParseResponse.self, from: data),
              decoded.error == nil,
              let title = decoded.title,
              !title.isEmpty
        else {
            return nil
        }
        return title
    }

    private static func roleName(_ role: ChatMessage.Role) -> String {
        switch role {
        case .user: return "user"
        case .assistant: return "assistant"
        case .toolBatch: return "tool_batch"
        case .error: return "error"
        case .skillActivated: return "skill_activated"
        }
    }
}
