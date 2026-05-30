import Foundation

// MARK: - Display helpers

extension Settings {

    /// Returns a human-readable display name for an OpenRouter model.
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

    mutating func markOpenRouterManual(connectedAt: Date = Date()) {
        openRouterCredentialSource = .manual
        openRouterBYOKKeyID = nil
        openRouterBYOKKeyLabel = nil
        openRouterConnectedAt = connectedAt
    }

    mutating func markOpenRouterBYOK(keyID: String?, keyLabel: String?, connectedAt: Date = Date()) {
        openRouterCredentialSource = .byok
        openRouterBYOKKeyID = keyID
        openRouterBYOKKeyLabel = keyLabel
        openRouterConnectedAt = connectedAt
    }

    mutating func clearOpenRouterCredential() {
        openRouterCredentialSource = .none
        openRouterBYOKKeyID = nil
        openRouterBYOKKeyLabel = nil
        openRouterConnectedAt = nil
    }

    mutating func markOllamaManual(connectedAt: Date = Date()) {
        ollamaCredentialSource = .manual
        ollamaBYOKKeyID = nil
        ollamaBYOKKeyLabel = nil
        ollamaConnectedAt = connectedAt
    }

    mutating func markOllamaBYOK(keyID: String?, keyLabel: String?, connectedAt: Date = Date()) {
        ollamaCredentialSource = .byok
        ollamaBYOKKeyID = keyID
        ollamaBYOKKeyLabel = keyLabel
        ollamaConnectedAt = connectedAt
    }

    mutating func clearOllamaCredential() {
        ollamaCredentialSource = .none
        ollamaBYOKKeyID = nil
        ollamaBYOKKeyLabel = nil
        ollamaConnectedAt = nil
    }

    mutating func markElevenLabsManual(connectedAt: Date = Date()) {
        elevenLabsCredentialSource = .manual
        elevenLabsBYOKKeyID = nil
        elevenLabsBYOKKeyLabel = nil
        elevenLabsConnectedAt = connectedAt
    }

    mutating func markElevenLabsBYOK(keyID: String?, keyLabel: String?, connectedAt: Date = Date()) {
        elevenLabsCredentialSource = .byok
        elevenLabsBYOKKeyID = keyID
        elevenLabsBYOKKeyLabel = keyLabel
        elevenLabsConnectedAt = connectedAt
    }

    mutating func clearElevenLabsCredential() {
        elevenLabsCredentialSource = .none
        elevenLabsBYOKKeyID = nil
        elevenLabsBYOKKeyLabel = nil
        elevenLabsConnectedAt = nil
    }
}
