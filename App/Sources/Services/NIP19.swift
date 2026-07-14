import Foundation

// MARK: - NIP-19 naddr encoder
//
// Encodes parameterised replaceable Nostr events (NIP-33) as bech32 `naddr`
// strings using TLV (Type-Length-Value) encoding per the NIP-19 spec.
//
// TLV type assignments:
//   0 — identifier (d-tag value, UTF-8)
//   1 — relay URL hint (UTF-8, optional)
//   2 — pubkey raw bytes (32 bytes decoded from hex)
//   3 — kind (4-byte big-endian UInt32)

enum NIP19 {
    /// Encode a parameterised replaceable event as an `naddr` bech32 string.
    ///
    /// - Parameters:
    ///   - dTag: Full d-tag string, e.g. `"podcast:guid:<uuid-lowercase>"`.
    ///   - pubkeyHex: 64-character lowercase hex x-only pubkey.
    ///   - kind: Nostr event kind (e.g. 30074 or 30075).
    ///   - relayURL: Optional relay hint included as TLV type-1.
    /// - Returns: `nil` when `pubkeyHex` cannot be decoded to exactly 32 bytes.
    static func naddr(
        dTag: String,
        pubkeyHex: String,
        kind: UInt32,
        relayURL: String? = nil
    ) -> String? {
        guard let pubkeyBytes = Data(hexString: pubkeyHex), pubkeyBytes.count == 32 else {
            return nil
        }
        var tlv = Data()

        // Type 0: d-tag (UTF-8)
        let dTagBytes = Data(dTag.utf8)
        tlv.append(contentsOf: [0, UInt8(dTagBytes.count)])
        tlv.append(contentsOf: dTagBytes)

        // Type 1: relay URL (UTF-8, optional)
        if let relay = relayURL {
            let relayBytes = Data(relay.utf8)
            if relayBytes.count <= 255 {
                tlv.append(contentsOf: [1, UInt8(relayBytes.count)])
                tlv.append(contentsOf: relayBytes)
            }
        }

        // Type 2: pubkey (32 raw bytes)
        tlv.append(contentsOf: [2, 32])
        tlv.append(contentsOf: pubkeyBytes)

        // Type 3: kind (4-byte big-endian)
        tlv.append(contentsOf: [3, 4])
        tlv.append(UInt8((kind >> 24) & 0xFF))
        tlv.append(UInt8((kind >> 16) & 0xFF))
        tlv.append(UInt8((kind >> 8) & 0xFF))
        tlv.append(UInt8(kind & 0xFF))

        return Bech32.encode(hrp: "naddr", data: tlv)
    }
}
