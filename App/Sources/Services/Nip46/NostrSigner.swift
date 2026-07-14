import CryptoKit
import Foundation
import P256K

// MARK: - Event types

/// Unsigned event draft as accepted by `NostrSigner.sign(_:)`.
/// Mirrors the NIP-01 event JSON minus `id` / `pubkey` / `sig` (the signer fills those).
struct NostrEventDraft: Sendable, Equatable {
    var kind: Int
    var content: String
    var tags: [[String]]
    /// `created_at` UNIX seconds. Defaults to "now" at construction.
    var createdAt: Int

    init(kind: Int, content: String, tags: [[String]] = [], createdAt: Int = Int(Date().timeIntervalSince1970)) {
        self.kind = kind
        self.content = content
        self.tags = tags
        self.createdAt = createdAt
    }
}

/// Fully-signed Nostr event ready to publish.
struct SignedNostrEvent: Sendable, Equatable, Codable {
    let id: String        // 32-byte hex SHA-256 of the canonical [0, pubkey, created_at, kind, tags, content].
    let pubkey: String    // 32-byte hex x-only pubkey.
    let created_at: Int
    let kind: Int
    let tags: [[String]]
    let content: String
    let sig: String       // 64-byte hex Schnorr signature.
}

// MARK: - Signer protocol

/// Anything that can produce a Nostr signature. Lets the rest of the app stay agnostic
/// of whether the user is signing locally (`LocalKeySigner`) or via a remote bunker
/// over NIP-46 (`RemoteSigner`).
protocol NostrSigner: Sendable {
    /// The user-facing pubkey this signer publishes events under (32-byte hex x-only).
    func publicKey() async throws -> String
    /// Compute the canonical event id, sign it, and return the wire-ready event.
    func sign(_ draft: NostrEventDraft) async throws -> SignedNostrEvent
}

// MARK: - Local-key signer (current behaviour)

/// `NostrSigner` backed by an in-process secp256k1 key pair (the existing nsec flow).
struct LocalKeySigner: NostrSigner {
    let keyPair: NostrKeyPair

    func publicKey() async throws -> String { keyPair.publicKeyHex }

    func sign(_ draft: NostrEventDraft) async throws -> SignedNostrEvent {
        let pubkey = keyPair.publicKeyHex
        let id = try EventID.compute(
            pubkey: pubkey,
            createdAt: draft.createdAt,
            kind: draft.kind,
            tags: draft.tags,
            content: draft.content
        )
        let sig = try schnorrSign(messageHex: id, privateKeyHex: keyPair.privateKeyHex)
        return SignedNostrEvent(
            id: id,
            pubkey: pubkey,
            created_at: draft.createdAt,
            kind: draft.kind,
            tags: draft.tags,
            content: draft.content,
            sig: sig
        )
    }

    private func schnorrSign(messageHex: String, privateKeyHex: String) throws -> String {
        guard let msg = Data(hexString: messageHex), msg.count == 32,
              let privBytes = Data(hexString: privateKeyHex), privBytes.count == 32 else {
            throw NostrSignerError.invalidEventForSigning
        }
        let key = try P256K.Schnorr.PrivateKey(dataRepresentation: privBytes)
        var msgBytes = [UInt8](msg)
        var aux = [UInt8](repeating: 0, count: 32)
        // Fill aux with random bytes (BIP-340 recommends fresh randomness per signature).
        for i in 0..<32 { aux[i] = UInt8.random(in: .min ... .max) }
        let signature = try key.signature(message: &msgBytes, auxiliaryRand: &aux)
        return Data(signature.dataRepresentation).hexString
    }
}

// MARK: - Event ID

/// Canonical NIP-01 event id: `SHA256(JSON([0, pubkey, created_at, kind, tags, content]))`,
/// with the JSON serialized in a deterministic, no-whitespace form (and UTF-8 escapes per spec).
enum EventID {
    static func compute(pubkey: String, createdAt: Int, kind: Int, tags: [[String]], content: String) throws -> String {
        let canonical = canonicalJSON([0, pubkey, createdAt, kind, tags, content])
        guard let data = canonical.data(using: .utf8) else { throw NostrSignerError.invalidEventForSigning }
        let hash = SHA256.hash(data: data)
        return Data(hash).hexString
    }

    /// JSON-serialize the canonical NIP-01 array. We hand-roll this rather than using
    /// `JSONSerialization` to avoid implementation-defined whitespace and Foundation's
    /// over-aggressive escaping (it escapes `/` which is not what NIP-01 asks for, and
    /// is order-unstable for dictionaries).
    static func canonicalJSON(_ value: Any) -> String {
        switch value {
        case let n as Int: return String(n)
        case let s as String: return jsonString(s)
        case let arr as [Any]:
            let parts = arr.map { canonicalJSON($0) }
            return "[" + parts.joined(separator: ",") + "]"
        case let arr as [[String]]:
            let parts = arr.map { canonicalJSON($0) }
            return "[" + parts.joined(separator: ",") + "]"
        default:
            // Fallback through JSONSerialization for anything exotic (shouldn't happen for events).
            if let data = try? JSONSerialization.data(withJSONObject: value, options: []),
               let s = String(data: data, encoding: .utf8) { return s }
            return "null"
        }
    }

    /// JSON-string-escape per NIP-01: backslash escape `"`, `\`, and the C0 controls
    /// `\b`, `\t`, `\n`, `\f`, `\r`; other controls become `\u00XX`. No other escapes.
    private static func jsonString(_ s: String) -> String {
        var out = "\""
        out.reserveCapacity(s.utf8.count + 2)
        for scalar in s.unicodeScalars {
            switch scalar {
            case "\"": out.append("\\\"")
            case "\\": out.append("\\\\")
            case "\u{08}": out.append("\\b")
            case "\u{09}": out.append("\\t")
            case "\u{0A}": out.append("\\n")
            case "\u{0C}": out.append("\\f")
            case "\u{0D}": out.append("\\r")
            default:
                if scalar.value < 0x20 {
                    out.append(String(format: "\\u%04x", scalar.value))
                } else {
                    out.append(Character(scalar))
                }
            }
        }
        out.append("\"")
        return out
    }
}

enum NostrSignerError: LocalizedError {
    case invalidEventForSigning
    case remoteRejected(String)
    case timedOut
    case notConnected
    case missingPublicKey

    var errorDescription: String? {
        switch self {
        case .invalidEventForSigning: "Could not sign — event payload is invalid."
        case .remoteRejected(let m): "Remote signer rejected the request: \(m)"
        case .timedOut: "Remote signer did not respond in time."
        case .notConnected: "Remote signer is not connected."
        case .missingPublicKey: "Remote signer has not advertised a public key yet."
        }
    }
}
