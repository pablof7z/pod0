import CryptoKit
import Foundation
import P256K

/// A Nostr key pair (secp256k1). Stores only hex bytes — the live signing key
/// is reconstructed per-operation so this type is `Sendable`.
struct NostrKeyPair: Sendable {
    let privateKeyHex: String
    let publicKeyHex: String

    /// Expected byte length of a secp256k1 private key or x-only public key.
    private static let keyByteCount = 32
    /// Bech32 human-readable part for a Nostr private key.
    private static let nsecHRP = "nsec"

    private init(privateKeyHex: String, publicKeyHex: String) {
        self.privateKeyHex = privateKeyHex
        self.publicKeyHex = publicKeyHex
    }

    // MARK: - Init

    static func generate() throws -> NostrKeyPair {
        let key = try P256K.Schnorr.PrivateKey()
        return NostrKeyPair(
            privateKeyHex: Data(key.dataRepresentation).hexString,
            publicKeyHex: Data(key.xonly.bytes).hexString
        )
    }

    init(nsec: String) throws {
        guard let (hrp, bytes) = Bech32.decode(nsec),
              hrp == NostrKeyPair.nsecHRP,
              bytes.count == NostrKeyPair.keyByteCount else {
            throw NostrKeyPairError.invalidPrivateKey
        }
        let key = try P256K.Schnorr.PrivateKey(dataRepresentation: bytes)
        self.privateKeyHex = bytes.hexString
        self.publicKeyHex = Data(key.xonly.bytes).hexString
    }

    init(privateKeyHex: String) throws {
        guard let data = Data(hexString: privateKeyHex),
              data.count == NostrKeyPair.keyByteCount else {
            throw NostrKeyPairError.invalidPrivateKey
        }
        let key = try P256K.Schnorr.PrivateKey(dataRepresentation: data)
        self.privateKeyHex = privateKeyHex
        self.publicKeyHex = Data(key.xonly.bytes).hexString
    }

    // MARK: - Bech32 display

    /// Bech32-encoded private key (nsec…). Safe to force-unwrap: both initialisers
    /// validate hex and length, and `generate()` produces hex directly from raw bytes.
    var nsec: String { Bech32.encode(hrp: NostrKeyPair.nsecHRP, data: Data(hexString: privateKeyHex)!) }
    var npub: String { Bech32.encode(hrp: "npub", data: Data(hexString: publicKeyHex)!) }
}

enum NostrKeyPairError: LocalizedError {
    case invalidPrivateKey
    var errorDescription: String? { "Invalid Nostr private key" }
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
