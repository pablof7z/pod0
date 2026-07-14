import Foundation
import os.log

/// Stores the Nostr private key (hex) in Keychain.
/// The matching public key hex is stored in Settings (non-secret).
/// Implement secp256k1 key derivation (e.g. via swift-secp256k1) to derive pubkey from privkey.
enum NostrCredentialStore {
    private static let logger = Logger.app("NostrCredentialStore")
    private static let service = "\(Bundle.main.bundleIdentifier ?? "Podcastr").nostr"
    private static let account = "private-key-hex"

    static func savePrivateKey(_ hexKey: String) throws {
        let trimmed = hexKey.trimmed
        guard !trimmed.isEmpty else { return }
        try KeychainStore.saveString(trimmed, service: service, account: account)
    }

    static func privateKey() throws -> String? {
        guard let value = try KeychainStore.readString(service: service, account: account) else {
            return nil
        }
        let trimmed = value.trimmed
        return trimmed.isEmpty ? nil : trimmed
    }

    /// Returns `true` if a private key is stored in the Keychain.
    /// Keychain read errors are logged but treated as "no key present" to avoid
    /// crashing on transient entitlement or accessibility failures.
    static func hasPrivateKey() -> Bool {
        do {
            return try privateKey() != nil
        } catch {
            logger.error("hasPrivateKey: Keychain read failed — \(error, privacy: .public)")
            return false
        }
    }

    static func deletePrivateKey() throws {
        try KeychainStore.deleteString(service: service, account: account)
    }
}
