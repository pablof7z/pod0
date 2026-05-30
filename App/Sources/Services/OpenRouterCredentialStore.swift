import Foundation
import os.log

/// Thin shim over `PcstIdentityCapability` for the OpenRouter BYOK API key.
///
/// All reads and writes now go through the canonical `pcst.byok.openrouter`
/// Keychain slot managed by `PcstIdentityCapability`. The legacy
/// `<bundleID>.openrouter` service entry is migrated by
/// `LegacyKeychainMigration` on first launch.
enum OpenRouterCredentialStore {
    private static let accountID = PcstIdentityCapability.AccountID.byokOpenRouter
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
