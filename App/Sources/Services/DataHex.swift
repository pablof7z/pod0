import Foundation

// MARK: - Data ↔ hex string helpers
//
// Generic, dependency-free hex codec. Not crypto — it performs no key
// derivation or signing, only byte ↔ ASCII-hex conversion. Relocated here
// from the (deleted) Nostr key-pair file when key ownership moved to the
// kernel; call sites across the app still use it for generic byte identifiers
// such as blob hashes. Nostr pubkey/npub parsing and formatting is Rust-owned.

extension Data {
    /// Lowercase hex string of the bytes (`%02x` per byte).
    var hexString: String { map { String(format: "%02x", $0) }.joined() }

    /// Parse an even-length ASCII-hex string into bytes. Returns `nil` for odd
    /// length or any non-hex character.
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
