import Foundation
import os.log

// MARK: - Per-podcast NIP-F4 secret → Keychain migration (M6-part-B)
//
// Per-podcast Nostr secret keys (used by the NIP-F4 owned-podcast publish
// path in `nmp-app-podcast`) are persisted by Rust as plaintext in
// `<dataDir>/podcast-keys.json`. M6 moves those secrets into the iOS Keychain
// under the `pcst.identity.capability` namespace, one item per podcast keyed
// `pcst.podcast.<podcast_id>.nipf4`.
//
// ## Why Swift-driven (not a Rust FFI capability dispatch)
//
// The kernel's capability callback (`SyncCapabilityBridge`) routes only
// http/audio/download; `pcst.identity.capability` is NOT reachable from Rust
// (that contract — PD-019 — is deliberately unbuilt). App-domain secret I/O
// is Swift-owned today: `OpenRouter`/`Ollama`/`NostrCredentialStore` all write
// the Keychain directly via `PcstIdentityCapability.direct`. This migration
// follows that established pattern and the run-once shape of
// `LegacyKeychainMigration`.
//
// ## Idempotent, no sentinel
//
// Rust still writes `podcast-keys.json` as the source of truth this window, so
// a key minted during a session is picked up on the next launch's sync. The
// Keychain `store` op overwrites, so re-running every launch is a harmless
// upsert — no `UserDefaults` sentinel needed (and Rust couldn't set one
// anyway). The JSON file is intentionally left in place as the fallback; M7
// flips the Rust read path to Keychain-first and removes it.
//
// Doctrine D6: every failure is logged data, never a thrown error across the
// migration boundary.

/// One-shot (per launch) JSON→Keychain sync for per-podcast NIP-F4 secrets.
enum PodcastKeysKeychainMigration {

    private static let logger = Logger.app("PodcastKeysKeychainMigration")

    /// File name Rust writes — mirrors `store::podcast_keys::PODCAST_KEYS_FILE`.
    static let fileName = "podcast-keys.json"

    /// Build the Keychain `account_id` for a per-podcast secret. MUST match
    /// the convention the Rust read path will use in M7:
    /// `pcst.podcast.<podcast_id>.nipf4`.
    static func accountID(forPodcastID podcastID: String) -> String {
        "pcst.podcast.\(podcastID).nipf4"
    }

    // MARK: - Wire shape (mirrors Rust serde output)

    /// On-disk row — matches Rust `PersistedKey` field names exactly
    /// (`podcast_id`, `secret_hex`).
    struct PersistedKey: Decodable, Equatable {
        let podcastID: String
        let secretHex: String

        enum CodingKeys: String, CodingKey {
            case podcastID = "podcast_id"
            case secretHex = "secret_hex"
        }
    }

    /// On-disk envelope — matches Rust `PersistedKeys` (`schema_version`,
    /// `keys`). Only schema version 1 is understood.
    struct PersistedKeys: Decodable, Equatable {
        let schemaVersion: UInt32
        let keys: [PersistedKey]

        enum CodingKeys: String, CodingKey {
            case schemaVersion = "schema_version"
            case keys
        }
    }

    /// Schema version Rust currently writes. An unknown version yields an
    /// empty parse (treated as nothing to migrate) rather than a crash.
    static let supportedSchemaVersion: UInt32 = 1

    // MARK: - Pure parse (testable without Keychain)

    /// Decode `podcast-keys.json` bytes into `(podcastID, secretHex)` rows.
    ///
    /// Returns `[]` for malformed JSON, an unknown schema version, or
    /// secrets that aren't 64-char lowercase hex (defensive — a corrupt row
    /// must not poison the rest of the batch). Never throws (D6).
    static func parse(_ data: Data) -> [(podcastID: String, secretHex: String)] {
        guard let payload = try? JSONDecoder().decode(PersistedKeys.self, from: data) else {
            return []
        }
        guard payload.schemaVersion == supportedSchemaVersion else {
            logger.error("unknown podcast-keys schema version \(payload.schemaVersion, privacy: .public) — skipping migration")
            return []
        }
        return payload.keys.compactMap { row in
            guard isValidSecretHex(row.secretHex) else {
                logger.error("skipping malformed secret_hex for podcast \(row.podcastID, privacy: .public)")
                return nil
            }
            return (row.podcastID, row.secretHex)
        }
    }

    /// 64 lowercase hex chars — the exact form Rust emits via `secret_to_hex`.
    static func isValidSecretHex(_ hex: String) -> Bool {
        hex.count == 64 && hex.allSatisfy { $0.isHexDigit && !$0.isUppercase }
    }

    // MARK: - Migration (Keychain side-effect)

    /// Sync every secret in `<dataDir>/podcast-keys.json` into the Keychain.
    ///
    /// Idempotent upsert; safe to call on every launch. `save` defaults to
    /// the canonical `PcstIdentityCapability.direct.saveSecret`, but is
    /// injectable so the batch logic can be unit-tested without a Keychain.
    /// Returns the number of secrets successfully written.
    @discardableResult
    static func runIfNeeded(
        dataDir: URL,
        save: (_ secretHex: String, _ accountID: String) throws -> Void = { secretHex, accountID in
            try PcstIdentityCapability.direct.saveSecret(secretHex, for: accountID)
        }
    ) -> Int {
        let fileURL = dataDir.appendingPathComponent(fileName, isDirectory: false)
        guard let data = try? Data(contentsOf: fileURL) else {
            // Missing file is the common case (no owned podcasts) — not an error.
            return 0
        }
        let rows = parse(data)
        guard !rows.isEmpty else { return 0 }

        var migrated = 0
        for row in rows {
            let account = accountID(forPodcastID: row.podcastID)
            do {
                try save(row.secretHex, account)
                migrated += 1
            } catch {
                logger.error("failed to store podcast secret for \(row.podcastID, privacy: .public): \(error.localizedDescription, privacy: .public)")
            }
        }
        if migrated > 0 {
            logger.info("migrated \(migrated)/\(rows.count) per-podcast secret(s) into the Keychain")
        }
        return migrated
    }
}
