import CryptoKit
import Foundation
import Pod0Core

extension SharedLibraryBootstrap {
    static func stableID(_ seed: String) -> CommandId {
        let digest = Array(SHA256.hash(data: Data(seed.utf8)))
        let high = digest[0..<8].reduce(UInt64(0)) { ($0 << 8) | UInt64($1) }
        let low = digest[8..<16].reduce(UInt64(0)) { ($0 << 8) | UInt64($1) }
        return CommandId(high: high, low: low)
    }

    static func stableDigest(_ seed: String) -> ContentDigest {
        let digest = Array(SHA256.hash(data: Data(seed.utf8)))
        func word(_ offset: Int) -> UInt64 {
            digest[offset..<(offset + 8)].reduce(UInt64(0)) {
                ($0 << 8) | UInt64($1)
            }
        }
        return ContentDigest(
            word0: word(0),
            word1: word(8),
            word2: word(16),
            word3: word(24)
        )
    }
}
