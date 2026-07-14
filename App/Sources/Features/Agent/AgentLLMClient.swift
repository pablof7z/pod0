import Foundation

enum AgentLLMClient {
    @MainActor
    static func streamCompletion(
        messages: [[String: Any]],
        tools: [[String: Any]],
        model: String,
        feature: String = CostFeature.agentChat,
        ollamaChatURL: URL? = nil,
        onPartialContent: (String) -> Void
    ) async throws -> AgentResult {
        let reference = LLMModelReference(storedID: model)
        guard !reference.isEmpty else {
            throw AgentError.httpError("No model selected.")
        }
        guard let apiKey = try LLMProviderCredentialResolver.apiKey(for: reference.provider),
              !apiKey.isEmpty else {
            throw AgentError.httpError(
                LLMProviderCredentialResolver.missingCredentialMessage(for: reference.provider)
            )
        }

        switch reference.provider {
        case .openRouter:
            return try await AgentOpenRouterClient.streamCompletion(
                messages: messages,
                tools: tools,
                apiKey: apiKey,
                model: reference.modelID,
                feature: feature,
                onPartialContent: onPartialContent
            )
        case .ollama:
            return try await AgentOllamaClient.streamCompletion(
                messages: messages,
                tools: tools,
                apiKey: apiKey,
                model: reference.modelID,
                feature: feature,
                chatURL: ollamaChatURL,
                onPartialContent: onPartialContent
            )
        }
    }
}
