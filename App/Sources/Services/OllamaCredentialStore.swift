import Foundation
import os.log

/// Thin shim over `PcstIdentityCapability` for the Ollama BYOK API key.
///
/// All reads and writes now go through the canonical `pcst.byok.ollama`
/// Keychain slot managed by `PcstIdentityCapability`. The legacy
/// `<bundleID>.ollama` service entry is migrated by
/// `LegacyKeychainMigration` v2 on first launch post-M3.
enum OllamaCredentialStore {
    private static let accountID = PcstIdentityCapability.AccountID.byokOllama
    private static var identity: PcstIdentityCapability { PcstIdentityCapability.direct }

    static func saveAPIKey(_ apiKey: String) throws {
        let trimmed = apiKey.trimmed
        guard !trimmed.isEmpty else { return }
        try identity.saveSecret(trimmed, for: accountID)
    }

    static func apiKey() throws -> String? {
        guard let value = try identity.loadSecret(for: accountID) else { return nil }
        let trimmed = value.trimmed
        return trimmed.isEmpty ? nil : trimmed
    }

    static func hasAPIKey() -> Bool {
        identity.hasSecret(for: accountID)
    }

    static func deleteAPIKey() throws {
        try identity.deleteSecret(for: accountID)
    }
}

enum LLMProviderCredentialResolver {
    static func apiKey(for provider: LLMProvider) throws -> String? {
        switch provider {
        case .openRouter:
            return try OpenRouterCredentialStore.apiKey()
        case .ollama:
            return try OllamaCredentialStore.apiKey()
        }
    }

    static func hasAPIKey(for provider: LLMProvider) -> Bool {
        switch provider {
        case .openRouter:
            return OpenRouterCredentialStore.hasAPIKey()
        case .ollama:
            return OllamaCredentialStore.hasAPIKey()
        }
    }

    /// Whether a non-nil API key is required for the given provider.
    ///
    /// Ollama running on-premises (localhost or any non-ollama.com host)
    /// does not require an API key — the key is only mandatory for the
    /// hosted cloud endpoint. All other providers always require a key.
    static func requiresAPIKey(for provider: LLMProvider, ollamaChatURL: URL? = nil) -> Bool {
        switch provider {
        case .openRouter:
            return true
        case .ollama:
            guard let host = ollamaChatURL?.host?.lowercased() else {
                return true
            }
            return host == "ollama.com" || host == "www.ollama.com"
        }
    }

    static func missingCredentialMessage(for provider: LLMProvider) -> String {
        "\(provider.displayName) is not connected. Add a key in Settings."
    }
}
