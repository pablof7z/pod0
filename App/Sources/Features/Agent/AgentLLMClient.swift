import Foundation

enum AgentLLMClient {
    @MainActor
    static func streamCompletion(
        messages: [[String: Any]],
        tools: [[String: Any]],
        model: String,
        store: AppStateStore,
        feature: String = CostFeature.agentChat,
        onPartialContent: (String) -> Void
    ) async throws -> AgentResult {
        let reference = LLMModelReference(storedID: model)
        guard !reference.isEmpty else {
            throw AgentError.httpError("No model selected.")
        }

        // Resolve credentials from Keychain based on provider.
        let apiKey = (try LLMProviderCredentialResolver.apiKey(for: reference.provider)) ?? ""

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
            let ollamaChatURL = URL(string: store.state.settings.ollamaChatURL)
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
