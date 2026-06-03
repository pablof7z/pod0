import Foundation

/// Provider-blind LLM readiness gate. Callers check readiness without
/// knowing which provider (OpenRouter, Ollama, etc.) is selected.
///
/// The kernel owns provider selection + credential resolution internally;
/// the factory injects the resolved key into the backend at dispatch time.
enum LLMReadiness {
    /// Check whether the selected model can be dispatched without user
    /// intervention (credential is available, provider is reachable, etc.).
    /// Returns `false` when the credential is required but missing.
    /// Must be called from the MainActor.
    @MainActor
    static func canSend(model: String, store: AppStateStore) -> Bool {
        let reference = LLMModelReference(storedID: model)
        let ollamaChatURL = URL(string: store.state.settings.ollamaChatURL)

        // Resolve whether a credential is required for this provider/endpoint.
        let requiresKey = requiresAPIKey(for: reference, ollamaChatURL: ollamaChatURL)
        guard requiresKey else { return true }

        // Credential is required — check if it's available in Keychain.
        return hasAPIKey(for: reference)
    }

    /// Human-readable error message explaining why the model cannot be
    /// dispatched (missing credential, provider offline, etc.).
    @MainActor
    static func missingCredentialMessage(for model: String) -> String {
        let reference = LLMModelReference(storedID: model)
        return "\(reference.provider.displayName) is not connected. Add a key in Settings."
    }

    // MARK: - Private helpers

    /// Whether a non-nil API key is required for the given provider.
    ///
    /// Ollama running on-premises (localhost or any non-ollama.com host)
    /// does not require an API key — the key is only mandatory for the
    /// hosted cloud endpoint. All other providers always require a key.
    private static func requiresAPIKey(
        for reference: LLMModelReference,
        ollamaChatURL: URL?
    ) -> Bool {
        switch reference.provider {
        case .openRouter:
            return true
        case .ollama:
            guard let host = ollamaChatURL?.host?.lowercased() else {
                return true
            }
            return host == "ollama.com" || host == "www.ollama.com"
        }
    }

    /// Whether the provider has an API key stored in Keychain.
    private static func hasAPIKey(for reference: LLMModelReference) -> Bool {
        switch reference.provider {
        case .openRouter:
            return OpenRouterCredentialStore.hasAPIKey()
        case .ollama:
            return OllamaCredentialStore.hasAPIKey()
        }
    }
}
