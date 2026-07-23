import Foundation
import P256K

/// Native key material used only by the secure signer capability.
///
/// Rust owns signer identity and event composition. Swift generates and holds
/// the platform credential, then signs the exact 32-byte event id requested by
/// the shared core.
struct CoreNostrSignerKeyMaterial: Sendable {
    let privateKeyHex: String
    let publicKeyHex: String

    static func generate() throws -> Self {
        let key = try P256K.Schnorr.PrivateKey()
        return Self(
            privateKeyHex: Data(key.dataRepresentation).hexString,
            publicKeyHex: Data(key.xonly.bytes).hexString
        )
    }
}

extension Data {
    var hexString: String { map { String(format: "%02x", $0) }.joined() }

    init?(hexString: String) {
        let value = hexString.lowercased()
        guard value.count.isMultiple(of: 2) else { return nil }
        var bytes: [UInt8] = []
        bytes.reserveCapacity(value.count / 2)
        var index = value.startIndex
        while index < value.endIndex {
            let next = value.index(index, offsetBy: 2)
            guard let byte = UInt8(value[index..<next], radix: 16) else { return nil }
            bytes.append(byte)
            index = next
        }
        self = Data(bytes)
    }
}
