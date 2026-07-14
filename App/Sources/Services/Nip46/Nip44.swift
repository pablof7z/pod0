import CryptoKit
import Foundation
import P256K

/// NIP-44 v2 payload encryption used to wrap kind:24133 NIP-46 messages.
///
/// Wire format: `base64( 0x02 || nonce(32) || ciphertext || mac(32) )`.
/// Padding scheme: 2-byte big-endian length prefix, then plaintext UTF-8, zero-padded
/// up to a power-of-two-bucketed length.
///
/// Reference: https://github.com/nostr-protocol/nips/blob/master/44.md
enum Nip44 {
    static let version: UInt8 = 0x02
    /// NIP-44 v2 random nonce length (32 bytes — fed into HKDF-Expand as `info`).
    static let messageNonceSize = 32
    /// HMAC tag length (SHA-256).
    static let macSize = 32
    /// Minimum padded plaintext bucket.
    static let minPaddedSize = 32
    /// Maximum unencrypted plaintext bytes (NIP-44 v2 limit: 65,535 bytes).
    static let maxPlaintextSize = 65_535
    /// HKDF salt for the conversation-key extract step.
    static let conversationKeySalt = "nip44-v2".data(using: .utf8)!

    // MARK: - Conversation key (HKDF-Extract over ECDH X)

    /// `HKDF-Extract(salt: "nip44-v2", ikm: ecdh_shared_x)` → 32-byte conversation key.
    /// `ecdh_shared_x` is the **raw 32-byte X coordinate** of `priv_a × pub_b`.
    static func conversationKey(privateKeyHex: String, peerPublicKeyHex: String) throws -> Data {
        let privBytes = try requireHex(privateKeyHex, byteCount: 32, label: "private key")
        let xOnly = try requireHex(peerPublicKeyHex, byteCount: 32, label: "peer pubkey")

        // NIP-44 lifts the x-only pubkey to the even-Y full point (BIP-340 convention).
        var compressed = Data([0x02])
        compressed.append(xOnly)

        let priv = try P256K.KeyAgreement.PrivateKey(dataRepresentation: privBytes)
        let pub = try P256K.KeyAgreement.PublicKey(dataRepresentation: compressed, format: .compressed)
        let shared = priv.sharedSecretFromKeyAgreement(with: pub, format: .compressed)

        // SharedSecret is the SEC1 compressed encoding (33 bytes: prefix || X). Drop prefix.
        let sharedBytes: [UInt8] = shared.withUnsafeBytes { Array($0) }
        guard sharedBytes.count == 33 else { throw Nip44Error.ecdhFailed }
        let sharedX = Data(sharedBytes.dropFirst())

        let prk = CryptoKit.HKDF<CryptoKit.SHA256>.extract(
            inputKeyMaterial: CryptoKit.SymmetricKey(data: sharedX),
            salt: conversationKeySalt
        )
        return prk.withUnsafeBytes { Data($0) }
    }

    // MARK: - Encrypt

    static func encrypt(plaintext: String, conversationKey: Data, nonce: Data? = nil) throws -> String {
        guard conversationKey.count == 32 else { throw Nip44Error.invalidKeySize }
        let plaintextBytes = Data(plaintext.utf8)
        guard !plaintextBytes.isEmpty else { throw Nip44Error.emptyPlaintext }
        guard plaintextBytes.count <= maxPlaintextSize else { throw Nip44Error.plaintextTooLong }

        let messageNonce: Data
        if let nonce {
            guard nonce.count == messageNonceSize else { throw Nip44Error.invalidNonce }
            messageNonce = nonce
        } else {
            messageNonce = Data((0..<messageNonceSize).map { _ in UInt8.random(in: .min ... .max) })
        }

        let keys = derivePerMessageKeys(conversationKey: conversationKey, nonce: messageNonce)
        let padded = pad(plaintext: plaintextBytes)
        let ciphertext = ChaCha20.xor(key: keys.chachaKey, nonce: keys.chachaNonce, counter: 0, data: padded)
        let mac = hmacSHA256(key: keys.hmacKey, data: messageNonce + ciphertext)

        var payload = Data([version])
        payload.append(messageNonce)
        payload.append(ciphertext)
        payload.append(mac)
        return payload.base64EncodedString()
    }

    // MARK: - Decrypt

    static func decrypt(payload: String, conversationKey: Data) throws -> String {
        guard conversationKey.count == 32 else { throw Nip44Error.invalidKeySize }
        guard let data = Data(base64Encoded: payload) else { throw Nip44Error.invalidBase64 }
        // 1 (version) + 32 (nonce) + 32+ (ciphertext) + 32 (mac).
        guard data.count >= 1 + messageNonceSize + minPaddedSize + macSize else {
            throw Nip44Error.payloadTooShort
        }
        guard data[data.startIndex] == version else { throw Nip44Error.unsupportedVersion }

        let nonce = data.subdata(in: data.index(after: data.startIndex)..<data.index(data.startIndex, offsetBy: 1 + messageNonceSize))
        let macStart = data.endIndex - macSize
        let ciphertext = data.subdata(in: data.index(data.startIndex, offsetBy: 1 + messageNonceSize)..<macStart)
        let providedMac = data.subdata(in: macStart..<data.endIndex)

        let keys = derivePerMessageKeys(conversationKey: conversationKey, nonce: nonce)
        let expectedMac = hmacSHA256(key: keys.hmacKey, data: nonce + ciphertext)
        guard constantTimeEqual(expectedMac, providedMac) else { throw Nip44Error.invalidMac }

        let padded = ChaCha20.xor(key: keys.chachaKey, nonce: keys.chachaNonce, counter: 0, data: ciphertext)
        return try unpadThrowing(padded: padded)
    }

    // MARK: - Per-message KDF (HKDF-Expand)

    struct PerMessageKeys {
        let chachaKey: Data    // 32
        let chachaNonce: Data  // 12
        let hmacKey: Data      // 32
    }

    /// `HKDF-Expand(prk: conversation_key, info: nonce, L: 76)` → split 32 || 12 || 32.
    static func derivePerMessageKeys(conversationKey: Data, nonce: Data) -> PerMessageKeys {
        let prk = CryptoKit.SymmetricKey(data: conversationKey)
        let expanded = CryptoKit.HKDF<CryptoKit.SHA256>.expand(
            pseudoRandomKey: prk,
            info: nonce,
            outputByteCount: 76
        )
        let bytes: Data = expanded.withUnsafeBytes { Data($0) }
        return PerMessageKeys(
            chachaKey: bytes.subdata(in: 0..<32),
            chachaNonce: bytes.subdata(in: 32..<44),
            hmacKey: bytes.subdata(in: 44..<76)
        )
    }

    // MARK: - Padding

    /// NIP-44 v2 padded length:
    /// - if `len <= 32` → 32
    /// - else: nextpower = next power of two ≥ `len`; chunk = 32 if nextpower ≤ 256 else nextpower/8;
    ///         padded = chunk * ((len-1) / chunk + 1)
    static func calcPaddedLen(_ len: Int) -> Int {
        precondition(len > 0)
        if len <= 32 { return 32 }
        // floor(log2(len-1))+1 == bitWidth - leadingZeroBitCount of (len-1)
        let nextpower = 1 << (Int.bitWidth - (len - 1).leadingZeroBitCount)
        let chunk = nextpower <= 256 ? 32 : nextpower / 8
        return chunk * ((len - 1) / chunk + 1)
    }

    /// Plaintext serialization: `len(2 BE) || utf8 || 0x00 * (padded - len)`. Total = 2 + padded.
    private static func pad(plaintext: Data) -> Data {
        let len = plaintext.count
        let padded = calcPaddedLen(len)
        var out = Data(capacity: 2 + padded)
        out.append(UInt8(len >> 8))
        out.append(UInt8(len & 0xff))
        out.append(plaintext)
        if padded > len {
            out.append(Data(repeating: 0, count: padded - len))
        }
        return out
    }

    private static func unpadThrowing(padded: Data) throws -> String {
        guard padded.count >= 2 + minPaddedSize else { throw Nip44Error.paddingInvalid }
        let len = (Int(padded[padded.startIndex]) << 8) | Int(padded[padded.startIndex + 1])
        let bodyStart = padded.startIndex + 2
        guard len > 0, len <= padded.count - 2 else { throw Nip44Error.paddingInvalid }
        // Verify that 2 + padded(len) equals the actual buffer size.
        guard 2 + calcPaddedLen(len) == padded.count else { throw Nip44Error.paddingInvalid }
        let body = padded.subdata(in: bodyStart..<bodyStart + len)
        guard let str = String(data: body, encoding: .utf8) else { throw Nip44Error.notUTF8 }
        return str
    }

    // MARK: - HMAC + helpers

    private static func hmacSHA256(key: Data, data: Data) -> Data {
        let mac = CryptoKit.HMAC<CryptoKit.SHA256>.authenticationCode(for: data, using: CryptoKit.SymmetricKey(data: key))
        return Data(mac)
    }

    private static func constantTimeEqual(_ a: Data, _ b: Data) -> Bool {
        guard a.count == b.count else { return false }
        var diff: UInt8 = 0
        for i in 0..<a.count { diff |= a[a.startIndex + i] ^ b[b.startIndex + i] }
        return diff == 0
    }

    private static func requireHex(_ hex: String, byteCount: Int, label: String) throws -> Data {
        guard let data = Data(hexString: hex), data.count == byteCount else {
            throw Nip44Error.invalidHex(label)
        }
        return data
    }
}

enum Nip44Error: LocalizedError {
    case invalidKeySize
    case invalidNonce
    case invalidBase64
    case payloadTooShort
    case unsupportedVersion
    case invalidMac
    case paddingInvalid
    case notUTF8
    case emptyPlaintext
    case plaintextTooLong
    case ecdhFailed
    case invalidHex(String)

    var errorDescription: String? {
        switch self {
        case .invalidKeySize: "NIP-44 conversation key must be 32 bytes."
        case .invalidNonce: "NIP-44 nonce must be 32 bytes."
        case .invalidBase64: "NIP-44 payload is not valid base64."
        case .payloadTooShort: "NIP-44 payload is too short."
        case .unsupportedVersion: "Unsupported NIP-44 version (only v2 / 0x02 is supported)."
        case .invalidMac: "NIP-44 MAC verification failed."
        case .paddingInvalid: "NIP-44 padding is invalid."
        case .notUTF8: "NIP-44 plaintext is not valid UTF-8."
        case .emptyPlaintext: "NIP-44 plaintext must not be empty."
        case .plaintextTooLong: "NIP-44 plaintext exceeds 65,535 bytes."
        case .ecdhFailed: "secp256k1 ECDH did not return a 33-byte compressed point."
        case .invalidHex(let label): "Invalid hex for \(label)."
        }
    }
}
