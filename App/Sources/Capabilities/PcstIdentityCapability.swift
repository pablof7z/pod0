import Foundation
import Security

// MARK: - pcst.identity.capability
//
// Handles all podcast-app–private Keychain slots:
//
//   pcst.identity.nsec            — user Nostr private key (hex or bech32 nsec)
//   pcst.identity.bunker_session  — NIP-46 bunker session token
//   pcst.byok.openai              — BYOK OpenAI API key
//   pcst.byok.openrouter          — BYOK OpenRouter API key
//   pcst.byok.elevenlabs          — BYOK ElevenLabs API key
//
// The `account_id` field in the payload MUST match one of the constants
// declared in `AccountID` below. Unknown account IDs are rejected with
// `status == "error"` (D6 — never a thrown exception).
//
// Doctrines:
//   D6 — failures are data, never exceptions.
//   D7 — capability reports raw Keychain results; Rust decides policy.

/// iOS Keychain Services implementation of the podcast-app identity namespace.
///
/// Secrets are stored as `kSecClassGenericPassword` items accessible only
/// `kSecAttrAccessibleWhenUnlockedThisDeviceOnly`. Items never sync to iCloud
/// and are never migrated to a new device.
final class PcstIdentityCapability {
    static let namespace = "pcst.identity.capability"
    /// Non-MainActor singleton for direct Keychain access from non-isolated
    /// contexts (e.g. credential-store shims that are called from synchronous
    /// service code). This instance is distinct from the one held by
    /// `PodcastCapabilities` — Keychain operations are stateless so both
    /// instances operate on the same physical Keychain items.
    nonisolated(unsafe) static let direct = PcstIdentityCapability()

    /// `kSecAttrService` for all items managed by this capability.
    static let service = "io.f7z.podcast.pcst.identity"

    /// Exhaustive set of account IDs this capability owns.
    enum AccountID {
        static let nsec           = "pcst.identity.nsec"
        static let bunkerSession  = "pcst.identity.bunker_session"
        static let byokOpenAI     = "pcst.byok.openai"
        static let byokOpenRouter = "pcst.byok.openrouter"
        static let byokOllama     = "pcst.byok.ollama"
        static let byokElevenLabs = "pcst.byok.elevenlabs"

        static let all: Set<String> = [
            nsec, bunkerSession, byokOpenAI, byokOpenRouter, byokOllama, byokElevenLabs,
        ]
    }

    private var started = false

    // MARK: - Lifecycle (idempotent)

    func start() { started = true }

    /// Idempotent. Does NOT erase stored secrets (D7: erasing is policy).
    func stop() { started = false }

    var isStarted: Bool { started }

    // MARK: - Envelope handling (never throws — D6)

    /// Decode → validate → execute → encode.
    /// Any failure returns a populated error envelope; this method never throws.
    func handle(_ request: CapabilityRequest) -> CapabilityEnvelope {
        let result = process(request)
        let resultJSON = Self.encode(result)
            ?? Self.encode(KeyringResult.error(errSecParam))
            ?? "{\"status\":\"error\"}"
        return CapabilityEnvelope(
            namespace: Self.namespace,
            correlationID: request.correlationID,
            resultJSON: resultJSON)
    }

    /// Entry point for FFI bridges supplying raw JSON. Honors D6 end-to-end.
    func handleJSON(_ requestJSON: String) -> String {
        guard
            let data = requestJSON.data(using: .utf8),
            let request = try? JSONDecoder().decode(CapabilityRequest.self, from: data)
        else {
            let env = CapabilityEnvelope(
                namespace: Self.namespace,
                correlationID: "",
                resultJSON: Self.encode(KeyringResult.error(errSecParam)) ?? "{\"status\":\"error\"}")
            return Self.encode(env) ?? "{}"
        }
        return Self.encode(handle(request)) ?? "{}"
    }

    // MARK: - Internals

    private func process(_ request: CapabilityRequest) -> KeyringResult {
        guard started else { return .error(errSecNotAvailable) }
        guard
            let payload = request.payloadJSON.data(using: .utf8),
            let keyringRequest = try? JSONDecoder().decode(KeyringRequest.self, from: payload)
        else {
            return .error(errSecParam)
        }

        // Validate account_id against the allowlist.
        let accountID: String
        switch keyringRequest {
        case let .store(id, _): accountID = id
        case let .retrieve(id): accountID = id
        case let .delete(id):   accountID = id
        }
        guard AccountID.all.contains(accountID) else { return .error(errSecParam) }

        switch keyringRequest {
        case let .store(id, secret): return store(accountID: id, secret: secret)
        case let .retrieve(id):      return retrieve(accountID: id)
        case let .delete(id):        return delete(accountID: id)
        }
    }

    private func baseQuery(_ accountID: String) -> [String: Any] {
        [
            kSecClass as String:        kSecClassGenericPassword,
            kSecAttrService as String:  Self.service,
            kSecAttrAccount as String:  accountID,
        ]
    }

    // MARK: - Swift-native convenience API
    //
    // Direct Keychain access without FFI JSON round-trips.  Used by the credential
    // stores (OpenRouter, Ollama) that are wired into Swift-side UI code.

    /// Store a secret under `accountID`. Throws if the Keychain write fails.
    func saveSecret(_ secret: String, for accountID: String) throws {
        let r = store(accountID: accountID, secret: secret)
        if r.status == "error", let code = r.osStatus {
            throw NSError(domain: NSOSStatusErrorDomain, code: Int(code))
        }
    }

    /// Retrieve the secret for `accountID`, or `nil` when absent.
    func loadSecret(for accountID: String) throws -> String? {
        let r = retrieve(accountID: accountID)
        if r.status == "error", let code = r.osStatus {
            throw NSError(domain: NSOSStatusErrorDomain, code: Int(code))
        }
        return r.secret
    }

    /// `true` when the secret exists and is non-empty.
    func hasSecret(for accountID: String) -> Bool {
        let r = retrieve(accountID: accountID)
        return r.status == "ok" && r.secret != nil
    }

    /// Delete the secret for `accountID`. Ignores not-found.
    func deleteSecret(for accountID: String) throws {
        let r = delete(accountID: accountID)
        if r.status == "error", let code = r.osStatus {
            throw NSError(domain: NSOSStatusErrorDomain, code: Int(code))
        }
    }

    private func store(accountID: String, secret: String) -> KeyringResult {
        guard let data = secret.data(using: .utf8) else { return .error(errSecParam) }
        SecItemDelete(baseQuery(accountID) as CFDictionary)
        var attrs = baseQuery(accountID)
        attrs[kSecValueData as String]    = data
        attrs[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        let status = SecItemAdd(attrs as CFDictionary, nil)
        return status == errSecSuccess ? .ok() : .error(status)
    }

    private func retrieve(accountID: String) -> KeyringResult {
        var query = baseQuery(accountID)
        query[kSecReturnData as String]  = true
        query[kSecMatchLimit as String]  = kSecMatchLimitOne
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        switch status {
        case errSecSuccess:
            guard
                let raw = item as? Data,
                let secret = String(data: raw, encoding: .utf8)
            else { return .error(errSecDecode) }
            return .ok(secret: secret)
        case errSecItemNotFound:
            return .notFound
        default:
            return .error(status)
        }
    }

    private func delete(accountID: String) -> KeyringResult {
        let status = SecItemDelete(baseQuery(accountID) as CFDictionary)
        return (status == errSecSuccess || status == errSecItemNotFound)
            ? .ok()
            : .error(status)
    }

    private static func encode<T: Encodable>(_ value: T) -> String? {
        guard let data = try? JSONEncoder().encode(value) else { return nil }
        return String(data: data, encoding: .utf8)
    }
}
