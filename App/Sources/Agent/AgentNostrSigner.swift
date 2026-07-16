import CryptoKit
import Foundation
import P256K

/// Nonhuman agent signing seam. Human identity never uses this protocol.
protocol NostrSigner: Sendable {
    func publicKey() async throws -> String
    func sign(_ draft: NostrEventDraft) async throws -> SignedNostrEvent
}

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
        let signature = try schnorrSign(
            messageHex: id,
            privateKeyHex: keyPair.privateKeyHex
        )
        return SignedNostrEvent(
            id: id,
            pubkey: pubkey,
            created_at: draft.createdAt,
            kind: draft.kind,
            tags: draft.tags,
            content: draft.content,
            sig: signature
        )
    }

    private func schnorrSign(messageHex: String, privateKeyHex: String) throws -> String {
        guard let message = Data(hexString: messageHex), message.count == 32,
              let privateKey = Data(hexString: privateKeyHex), privateKey.count == 32 else {
            throw NostrSignerError.invalidEventForSigning
        }
        let key = try P256K.Schnorr.PrivateKey(dataRepresentation: privateKey)
        var messageBytes = [UInt8](message)
        var auxiliaryRandomness = [UInt8](repeating: 0, count: 32)
        for index in auxiliaryRandomness.indices {
            auxiliaryRandomness[index] = UInt8.random(in: .min ... .max)
        }
        let signature = try key.signature(
            message: &messageBytes,
            auxiliaryRand: &auxiliaryRandomness
        )
        return Data(signature.dataRepresentation).hexString
    }
}

enum EventID {
    static func compute(
        pubkey: String,
        createdAt: Int,
        kind: Int,
        tags: [[String]],
        content: String
    ) throws -> String {
        let canonical = canonicalJSON([0, pubkey, createdAt, kind, tags, content])
        guard let data = canonical.data(using: .utf8) else {
            throw NostrSignerError.invalidEventForSigning
        }
        return Data(SHA256.hash(data: data)).hexString
    }

    static func canonicalJSON(_ value: Any) -> String {
        switch value {
        case let number as Int:
            String(number)
        case let string as String:
            jsonString(string)
        case let array as [Any]:
            "[" + array.map(canonicalJSON).joined(separator: ",") + "]"
        case let array as [[String]]:
            "[" + array.map(canonicalJSON).joined(separator: ",") + "]"
        default:
            if let data = try? JSONSerialization.data(withJSONObject: value),
               let string = String(data: data, encoding: .utf8) {
                string
            } else {
                "null"
            }
        }
    }

    private static func jsonString(_ string: String) -> String {
        var output = "\""
        output.reserveCapacity(string.utf8.count + 2)
        for scalar in string.unicodeScalars {
            switch scalar {
            case "\"": output.append("\\\"")
            case "\\": output.append("\\\\")
            case "\u{08}": output.append("\\b")
            case "\u{09}": output.append("\\t")
            case "\u{0A}": output.append("\\n")
            case "\u{0C}": output.append("\\f")
            case "\u{0D}": output.append("\\r")
            default:
                if scalar.value < 0x20 {
                    output.append(String(format: "\\u%04x", scalar.value))
                } else {
                    output.append(Character(scalar))
                }
            }
        }
        output.append("\"")
        return output
    }
}

enum NostrSignerError: LocalizedError {
    case invalidEventForSigning

    var errorDescription: String? {
        "Could not sign — event payload is invalid."
    }
}
