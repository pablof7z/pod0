import Foundation
import Security

// MARK: - Legacy Keychain migration shim (M1.B)
//
// Runs once on first launch after the NMP migration. Reads items the legacy
// Podcastr app stored under the default per-app Keychain group (readable
// because the bundle ID is preserved as io.f7z.podcast — R6 pre-flight).
// Re-stores them under the canonical pcst.identity.capability accountIDs.
//
// Guard: UserDefaults key `pcst.migration.keychain.v1.done`. When set, this
// shim is skipped entirely; it is never re-run.
//
// Policy decisions (D7):
//   - If a legacy item is absent, the slot is skipped (not an error).
//   - If the new slot already has a value, the slot is skipped (no overwrite).
//   - The bunker_session slot receives the session private-key hex only; the
//     NIP-46 meta JSON is intentionally not migrated — the bunker URI changes
//     per session and Rust will prompt for re-pairing if the meta is absent.
//   - All failures are reported in the returned `MigrationReport`; the Rust
//     side decides whether to surface them to the user (D6).
//
// Doctrines:
//   D6 — failures are data in `MigrationReport`, never exceptions.
//   D7 — this shim performs mechanical Keychain reads and writes. It never
//         decides which key to use, whether to prompt the user, or whether to
//         clear the legacy slots (Rust decides via a follow-up capability call).

// MARK: - Migration report

/// Outcome of a single slot migration attempt.
struct SlotMigrationResult: Sendable {
    let accountID: String
    let outcome: Outcome

    enum Outcome: Sendable {
        case migrated
        case skippedAbsent
        case skippedAlreadyPresent
        case failed(OSStatus)
    }
}

/// Aggregate result reported to the Rust side after the migration runs.
struct MigrationReport: Sendable {
    let alreadyDone: Bool
    let slots: [SlotMigrationResult]
}

// MARK: - Migration executor

/// One-shot migration: reads legacy Keychain items and copies them into the
/// `pcst.identity.capability` namespace. Idempotent — guarded by UserDefaults.
enum LegacyKeychainMigration {

    // v1: covered nsec, bunkerSession, byokOpenRouter, byokElevenLabs
    private static let sentinelKeyV1 = "pcst.migration.keychain.v1.done"
    // v2: adds byokOllama (new slot added in M3). v2 re-runs the full batch so
    // the skip-if-already-present guard protects all slots already migrated in v1.
    private static let sentinelKeyV2 = "pcst.migration.keychain.v2.done"

    /// Legacy Keychain constants from the Podcastr app's credential stores.
    private enum Legacy {
        static let bundleID = Bundle.main.bundleIdentifier ?? "io.f7z.podcast"
        // UserIdentityStore
        static let userIdentityService = "\(bundleID).user-identity"
        static let userPrivateKeyAccount = "user-private-key-hex"
        // UserIdentityStore — NIP-46 session
        static let nip46SessionService = "\(bundleID).nip46-session"
        static let nip46SessionAccount = "session-private-key-hex"
        // OpenRouterCredentialStore
        static let openRouterService = "\(bundleID).openrouter"
        static let apiKeyAccount = "api-key"
        // ElevenLabsCredentialStore
        static let elevenLabsService = "\(bundleID).elevenlabs"
        // OllamaCredentialStore (added M3)
        static let ollamaService = "\(bundleID).ollama"
    }

    /// Run the migration. Safe to call on every launch; no-ops after the
    /// latest version's sentinel is set. v2 re-runs the full batch to pick up
    /// the Ollama slot; the skip-if-already-present guard is idempotent.
    @discardableResult
    static func runIfNeeded() -> MigrationReport {
        guard !UserDefaults.standard.bool(forKey: sentinelKeyV2) else {
            return MigrationReport(alreadyDone: true, slots: [])
        }

        let mapping: [(legacyService: String, legacyAccount: String, newAccountID: String)] = [
            (Legacy.userIdentityService,  Legacy.userPrivateKeyAccount, PcstIdentityCapability.AccountID.nsec),
            (Legacy.nip46SessionService,  Legacy.nip46SessionAccount,   PcstIdentityCapability.AccountID.bunkerSession),
            (Legacy.openRouterService,    Legacy.apiKeyAccount,          PcstIdentityCapability.AccountID.byokOpenRouter),
            (Legacy.elevenLabsService,    Legacy.apiKeyAccount,          PcstIdentityCapability.AccountID.byokElevenLabs),
            (Legacy.ollamaService,        Legacy.apiKeyAccount,          PcstIdentityCapability.AccountID.byokOllama),
        ]

        var results: [SlotMigrationResult] = []
        for entry in mapping {
            let result = migrateSlot(
                legacyService: entry.legacyService,
                legacyAccount: entry.legacyAccount,
                newAccountID: entry.newAccountID
            )
            results.append(result)
        }

        // Mark both v1 and v2 done so the check above works on a clean install.
        UserDefaults.standard.set(true, forKey: sentinelKeyV1)
        UserDefaults.standard.set(true, forKey: sentinelKeyV2)
        return MigrationReport(alreadyDone: false, slots: results)
    }

    // MARK: - Slot migration

    private static func migrateSlot(
        legacyService: String,
        legacyAccount: String,
        newAccountID: String
    ) -> SlotMigrationResult {

        // 1. Skip if the new slot already has a value (no overwrite — D7).
        switch readItem(service: PcstIdentityCapability.service, account: newAccountID) {
        case .found:
            return SlotMigrationResult(accountID: newAccountID, outcome: .skippedAlreadyPresent)
        case .notFound:
            break
        case .error(let status):
            return SlotMigrationResult(accountID: newAccountID, outcome: .failed(status))
        }

        // 2. Read the legacy item.
        let legacyValue: String
        switch readItem(service: legacyService, account: legacyAccount) {
        case .found(let value):
            legacyValue = value
        case .notFound:
            return SlotMigrationResult(accountID: newAccountID, outcome: .skippedAbsent)
        case .error(let status):
            return SlotMigrationResult(accountID: newAccountID, outcome: .failed(status))
        }

        // 3. Write to the new slot.
        let writeStatus = writeItem(
            service: PcstIdentityCapability.service,
            account: newAccountID,
            value: legacyValue
        )
        if writeStatus == errSecSuccess {
            return SlotMigrationResult(accountID: newAccountID, outcome: .migrated)
        } else {
            return SlotMigrationResult(accountID: newAccountID, outcome: .failed(writeStatus))
        }
    }

    // MARK: - Raw Keychain helpers (D6: no throws)

    private static func readItem(service: String, account: String) -> SecretLookup {
        let query: [String: Any] = [
            kSecClass as String:       kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String:  true,
            kSecMatchLimit as String:  kSecMatchLimitOne,
        ]
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        switch status {
        case errSecSuccess:
            guard
                let data = item as? Data,
                let value = String(data: data, encoding: .utf8)
            else { return .error(errSecDecode) }
            return .found(value)
        case errSecItemNotFound:
            return .notFound
        default:
            return .error(status)
        }
    }

    private static func writeItem(service: String, account: String, value: String) -> OSStatus {
        guard let data = value.data(using: .utf8) else { return errSecParam }
        let deleteQuery: [String: Any] = [
            kSecClass as String:       kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        SecItemDelete(deleteQuery as CFDictionary)
        let addAttrs: [String: Any] = [
            kSecClass as String:              kSecClassGenericPassword,
            kSecAttrService as String:        service,
            kSecAttrAccount as String:        account,
            kSecValueData as String:          data,
            kSecAttrAccessible as String:     kSecAttrAccessibleWhenUnlockedThisDeviceOnly,
        ]
        return SecItemAdd(addAttrs as CFDictionary, nil)
    }
}
