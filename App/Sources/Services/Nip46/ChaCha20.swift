import Foundation

/// Pure RFC 8439 ChaCha20 stream cipher (no Poly1305 — that's the AEAD form).
///
/// NIP-44 v2 needs *raw* ChaCha20 plus a separate HMAC-SHA256 of (nonce || ciphertext)
/// keyed with a derived MAC key. CryptoKit's `ChaChaPoly` is the AEAD construction and
/// produces a 16-byte tag we can't unbundle, so we implement RFC 8439 here from scratch.
///
/// - Parameters:
///   - key: 32 bytes.
///   - nonce: 12 bytes (IETF variant).
///   - counter: starting block counter (RFC 8439 conventionally starts at 1 for payload;
///     NIP-44 conforms to that convention).
enum ChaCha20 {
    static let keySize = 32
    static let nonceSize = 12
    static let blockSize = 64

    /// Encrypt or decrypt `data` with ChaCha20 (the cipher is symmetric — XOR keystream).
    static func xor(key: Data, nonce: Data, counter: UInt32 = 0, data: Data) -> Data {
        precondition(key.count == keySize, "ChaCha20 key must be 32 bytes")
        precondition(nonce.count == nonceSize, "ChaCha20 nonce must be 12 bytes")

        let keyWords = key.toLittleEndianWords(count: 8)
        let nonceWords = nonce.toLittleEndianWords(count: 3)

        var output = Data(count: data.count)
        var blockCounter = counter
        let inputBytes = [UInt8](data)
        var outputBytes = [UInt8](repeating: 0, count: data.count)

        var offset = 0
        while offset < inputBytes.count {
            let block = chachaBlock(key: keyWords, nonce: nonceWords, counter: blockCounter)
            let take = min(blockSize, inputBytes.count - offset)
            for i in 0..<take {
                outputBytes[offset + i] = inputBytes[offset + i] ^ block[i]
            }
            offset += take
            blockCounter &+= 1
        }
        output.replaceSubrange(0..<outputBytes.count, with: outputBytes)
        return output
    }

    // MARK: - Block function (RFC 8439 §2.3)

    private static func chachaBlock(key: [UInt32], nonce: [UInt32], counter: UInt32) -> [UInt8] {
        // "expand 32-byte k" — RFC 8439 constants
        var state: [UInt32] = [
            0x6170_7865, 0x3320_646e, 0x7962_2d32, 0x6b20_6574,
            key[0], key[1], key[2], key[3],
            key[4], key[5], key[6], key[7],
            counter, nonce[0], nonce[1], nonce[2],
        ]
        let initial = state

        for _ in 0..<10 {
            // Column rounds
            quarterRound(&state, 0, 4, 8, 12)
            quarterRound(&state, 1, 5, 9, 13)
            quarterRound(&state, 2, 6, 10, 14)
            quarterRound(&state, 3, 7, 11, 15)
            // Diagonal rounds
            quarterRound(&state, 0, 5, 10, 15)
            quarterRound(&state, 1, 6, 11, 12)
            quarterRound(&state, 2, 7, 8, 13)
            quarterRound(&state, 3, 4, 9, 14)
        }

        var bytes = [UInt8](repeating: 0, count: 64)
        for i in 0..<16 {
            let word = state[i] &+ initial[i]
            bytes[i * 4 + 0] = UInt8(truncatingIfNeeded: word)
            bytes[i * 4 + 1] = UInt8(truncatingIfNeeded: word >> 8)
            bytes[i * 4 + 2] = UInt8(truncatingIfNeeded: word >> 16)
            bytes[i * 4 + 3] = UInt8(truncatingIfNeeded: word >> 24)
        }
        return bytes
    }

    private static func quarterRound(_ s: inout [UInt32], _ a: Int, _ b: Int, _ c: Int, _ d: Int) {
        s[a] = s[a] &+ s[b]; s[d] = rotl32(s[d] ^ s[a], 16)
        s[c] = s[c] &+ s[d]; s[b] = rotl32(s[b] ^ s[c], 12)
        s[a] = s[a] &+ s[b]; s[d] = rotl32(s[d] ^ s[a], 8)
        s[c] = s[c] &+ s[d]; s[b] = rotl32(s[b] ^ s[c], 7)
    }

    private static func rotl32(_ x: UInt32, _ n: UInt32) -> UInt32 {
        (x << n) | (x >> (32 - n))
    }
}

// MARK: - Helpers

private extension Data {
    /// Pack consecutive 4-byte little-endian words.
    func toLittleEndianWords(count: Int) -> [UInt32] {
        precondition(self.count >= count * 4)
        var words = [UInt32](repeating: 0, count: count)
        for i in 0..<count {
            let base = i * 4
            words[i] = UInt32(self[startIndex + base]) |
                (UInt32(self[startIndex + base + 1]) << 8) |
                (UInt32(self[startIndex + base + 2]) << 16) |
                (UInt32(self[startIndex + base + 3]) << 24)
        }
        return words
    }
}
