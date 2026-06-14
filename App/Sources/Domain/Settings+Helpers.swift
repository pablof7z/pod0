import Foundation

// MARK: - Display helpers

extension Settings {

    /// Returns a human-readable display name for a provider model.
    ///
    /// Preference order:
    /// 1. `modelName` when non-empty (persisted human-readable name from catalog).
    /// 2. Slug after the last `/` in `modelID` (e.g. "gpt-4o" from "openai/gpt-4o").
    /// 3. `modelID` verbatim when it contains no `/`.
    /// 4. "Not set" when `modelID` is empty.
    static func modelDisplayName(modelID: String, modelName: String = "") -> String {
        let name = modelName.trimmed
        if !name.isEmpty { return name }
        let id = modelID.trimmed
        guard !id.isEmpty else { return "Not set" }
        let reference = LLMModelReference(storedID: id)
        if reference.provider != .openRouter { return reference.modelID }
        if let idx = id.lastIndex(of: "/") { return String(id[id.index(after: idx)...]) }
        return id
    }

    mutating func markOpenRouterManual() {
        openRouterCredentialSource = .manual
        openRouterBYOKKeyID = nil
        openRouterBYOKKeyLabel = nil
        // connected_at is stamped by the kernel (D9); do not set it here.
    }

    mutating func markOpenRouterBYOK(keyID: String?, keyLabel: String?) {
        openRouterCredentialSource = .byok
        openRouterBYOKKeyID = keyID
        openRouterBYOKKeyLabel = keyLabel
        // connected_at is stamped by the kernel (D9); do not set it here.
    }

    mutating func clearOpenRouterCredential() {
        openRouterCredentialSource = .none
        openRouterBYOKKeyID = nil
        openRouterBYOKKeyLabel = nil
        openRouterConnectedAt = nil
    }

    mutating func markOllamaManual() {
        ollamaCredentialSource = .manual
        ollamaBYOKKeyID = nil
        ollamaBYOKKeyLabel = nil
        // connected_at is stamped by the kernel (D9); do not set it here.
    }

    mutating func markOllamaBYOK(keyID: String?, keyLabel: String?) {
        ollamaCredentialSource = .byok
        ollamaBYOKKeyID = keyID
        ollamaBYOKKeyLabel = keyLabel
        // connected_at is stamped by the kernel (D9); do not set it here.
    }

    mutating func clearOllamaCredential() {
        ollamaCredentialSource = .none
        ollamaBYOKKeyID = nil
        ollamaBYOKKeyLabel = nil
        ollamaConnectedAt = nil
    }

    mutating func markElevenLabsManual() {
        elevenLabsCredentialSource = .manual
        elevenLabsBYOKKeyID = nil
        elevenLabsBYOKKeyLabel = nil
        // connected_at is stamped by the kernel (D9); do not set it here.
    }

    mutating func markElevenLabsBYOK(keyID: String?, keyLabel: String?) {
        elevenLabsCredentialSource = .byok
        elevenLabsBYOKKeyID = keyID
        elevenLabsBYOKKeyLabel = keyLabel
        // connected_at is stamped by the kernel (D9); do not set it here.
    }

    mutating func clearElevenLabsCredential() {
        elevenLabsCredentialSource = .none
        elevenLabsBYOKKeyID = nil
        elevenLabsBYOKKeyLabel = nil
        elevenLabsConnectedAt = nil
    }

    mutating func markAssemblyAIManual() {
        assemblyAICredentialSource = .manual
        assemblyAIBYOKKeyID = nil
        assemblyAIBYOKKeyLabel = nil
        // connected_at is stamped by the kernel (D9); do not set it here.
    }

    mutating func markAssemblyAIBYOK(keyID: String?, keyLabel: String?) {
        assemblyAICredentialSource = .byok
        assemblyAIBYOKKeyID = keyID
        assemblyAIBYOKKeyLabel = keyLabel
        // connected_at is stamped by the kernel (D9); do not set it here.
    }

    mutating func clearAssemblyAICredential() {
        assemblyAICredentialSource = .none
        assemblyAIBYOKKeyID = nil
        assemblyAIBYOKKeyLabel = nil
        assemblyAIConnectedAt = nil
    }

    mutating func markPerplexityManual() {
        perplexityCredentialSource = .manual
        perplexityBYOKKeyID = nil
        perplexityBYOKKeyLabel = nil
        // connected_at is stamped by the kernel (D9); do not set it here.
    }

    mutating func markPerplexityBYOK(keyID: String?, keyLabel: String?) {
        perplexityCredentialSource = .byok
        perplexityBYOKKeyID = keyID
        perplexityBYOKKeyLabel = keyLabel
        // connected_at is stamped by the kernel (D9); do not set it here.
    }

    mutating func clearPerplexityCredential() {
        perplexityCredentialSource = .none
        perplexityBYOKKeyID = nil
        perplexityBYOKKeyLabel = nil
        perplexityConnectedAt = nil
    }
}
