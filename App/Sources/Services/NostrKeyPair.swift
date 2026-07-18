import CryptoKit
import Foundation
import P256K

/// An ephemeral Nostr key pair used only to authorize a Blossom upload.
struct NostrKeyPair: Sendable {
    let privateKeyHex: String
    let publicKeyHex: String

    private init(privateKeyHex: String, publicKeyHex: String) {
        self.privateKeyHex = privateKeyHex
        self.publicKeyHex = publicKeyHex
    }

    static func generate() throws -> NostrKeyPair {
        let key = try P256K.Schnorr.PrivateKey()
        return NostrKeyPair(
            privateKeyHex: Data(key.dataRepresentation).hexString,
            publicKeyHex: Data(key.xonly.bytes).hexString
        )
    }

}

// MARK: - Hex helpers

extension Data {
    var hexString: String { map { String(format: "%02x", $0) }.joined() }

    init?(hexString: String) {
        let s = hexString.lowercased()
        guard s.count % 2 == 0 else { return nil }
        var bytes: [UInt8] = []
        bytes.reserveCapacity(s.count / 2)
        var idx = s.startIndex
        while idx < s.endIndex {
            let next = s.index(idx, offsetBy: 2)
            guard let b = UInt8(s[idx..<next], radix: 16) else { return nil }
            bytes.append(b)
            idx = next
        }
        self = Data(bytes)
    }
}
