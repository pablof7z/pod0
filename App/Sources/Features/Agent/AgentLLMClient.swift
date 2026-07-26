import Foundation

struct AgentTokenUsage: Codable, Sendable {
    let promptTokens: Int
    let completionTokens: Int
    let cachedTokens: Int?
}

struct AgentProviderPayload: @unchecked Sendable {
    let messages: [[String: Any]]
    let tools: [[String: Any]]
}

enum AgentLLMClient {
    static func streamCompletion(
        payload: AgentProviderPayload,
        model: String,
        feature: String = CostFeature.agentChat,
        ollamaChatURL: URL? = nil,
        onPartialContent: @escaping @MainActor @Sendable (String) -> Void
    ) async throws -> AgentResult {
        let reference = LLMModelReference(storedID: model)
        guard !reference.isEmpty else {
            throw AgentError.invalidInput
        }
        guard let apiKey = try LLMProviderCredentialResolver.apiKey(for: reference.provider),
              !apiKey.isEmpty else {
            throw AgentError.missingCredential
        }

        switch reference.provider {
        case .openRouter:
            return try await AgentOpenRouterClient.streamCompletion(
                messages: payload.messages,
                tools: payload.tools,
                apiKey: apiKey,
                model: reference.modelID,
                feature: feature,
                onPartialContent: onPartialContent
            )
        case .ollama:
            return try await AgentOllamaClient.streamCompletion(
                messages: payload.messages,
                tools: payload.tools,
                apiKey: apiKey,
                model: reference.modelID,
                feature: feature,
                chatURL: ollamaChatURL,
                onPartialContent: onPartialContent
            )
        }
    }
}
