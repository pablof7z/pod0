import Foundation
import Pod0Core

extension EpisodeId {
    init(uuid: UUID) {
        let hexadecimal = uuid.uuidString.replacingOccurrences(of: "-", with: "")
        self.init(
            high: UInt64(hexadecimal.prefix(16), radix: 16)!,
            low: UInt64(hexadecimal.suffix(16), radix: 16)!
        )
    }

    var uuid: UUID? {
        let hexadecimal = String(format: "%016llX%016llX", high, low)
        let formatted = [
            hexadecimal.prefix(8),
            hexadecimal.dropFirst(8).prefix(4),
            hexadecimal.dropFirst(12).prefix(4),
            hexadecimal.dropFirst(16).prefix(4),
            hexadecimal.dropFirst(20),
        ].map(String.init).joined(separator: "-")
        return UUID(uuidString: formatted)
    }
}

extension UnixTimestampMilliseconds {
    init(date: Date) {
        let value = Int64((date.timeIntervalSince1970 * 1_000).rounded())
        self.init(value: value)
    }

    var date: Date {
        Date(timeIntervalSince1970: Double(value) / 1_000)
    }
}
