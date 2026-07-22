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

extension PodcastId {
    init(uuid: UUID) {
        let parts = uuid.coreIdentifierParts
        self.init(high: parts.high, low: parts.low)
    }

    var uuid: UUID? { UUID(coreHigh: high, low: low) }
}

extension NoteId {
    init(uuid: UUID) {
        let parts = uuid.coreIdentifierParts
        self.init(high: parts.high, low: parts.low)
    }

    var uuid: UUID? { UUID(coreHigh: high, low: low) }
}

extension ClipId {
    init(uuid: UUID) {
        let parts = uuid.coreIdentifierParts
        self.init(high: parts.high, low: parts.low)
    }

    var uuid: UUID? { UUID(coreHigh: high, low: low) }
}

extension QueueEntryId {
    init(uuid: UUID) {
        let parts = uuid.coreIdentifierParts
        self.init(high: parts.high, low: parts.low)
    }

    var uuid: UUID? { UUID(coreHigh: high, low: low) }
}

extension ScheduledTaskId {
    init(uuid: UUID) {
        let parts = uuid.coreIdentifierParts
        self.init(high: parts.high, low: parts.low)
    }

    var uuid: UUID? { UUID(coreHigh: high, low: low) }
}

extension SpeakerId {
    init(uuid: UUID) {
        let hexadecimal = uuid.uuidString.replacingOccurrences(of: "-", with: "")
        self.init(
            high: UInt64(hexadecimal.prefix(16), radix: 16)!,
            low: UInt64(hexadecimal.suffix(16), radix: 16)!
        )
    }

    var uuid: UUID? { UUID(coreHigh: high, low: low) }
    var stableString: String { coreIdentifier(high: high, low: low) }
}

extension EvidenceGenerationId {
    var stableString: String { coreIdentifier(high: high, low: low) }
}

extension EvidenceSpanId {
    var stableString: String { coreIdentifier(high: high, low: low) }
}

extension TranscriptVersionId {
    var stableString: String { coreIdentifier(high: high, low: low) }
}

extension TranscriptArtifactId {
    var uuid: UUID? { UUID(coreHigh: high, low: low) }
    var stableString: String { coreIdentifier(high: high, low: low) }
}

extension TranscriptSegmentId {
    var uuid: UUID? { UUID(coreHigh: high, low: low) }
    var stableString: String { coreIdentifier(high: high, low: low) }
}

extension ChapterArtifactId {
    init(uuid: UUID) {
        let parts = uuid.coreIdentifierParts
        self.init(high: parts.high, low: parts.low)
    }

    var uuid: UUID? { UUID(coreHigh: high, low: low) }
    var stableString: String { coreIdentifier(high: high, low: low) }
}

extension ChapterId {
    var uuid: UUID? { UUID(coreHigh: high, low: low) }
}

extension DownloadIntentId {
    var stableString: String { coreIdentifier(high: high, low: low) }
}

extension AdSpanId {
    var uuid: UUID? { UUID(coreHigh: high, low: low) }
}

extension ContentDigest {
    init?(hexadecimal: String) {
        guard hexadecimal.count == 64,
              hexadecimal.utf8.allSatisfy({ byte in
                  (48...57).contains(byte) || (65...70).contains(byte) || (97...102).contains(byte)
              }),
              let word0 = UInt64(hexadecimal.prefix(16), radix: 16),
              let word1 = UInt64(hexadecimal.dropFirst(16).prefix(16), radix: 16),
              let word2 = UInt64(hexadecimal.dropFirst(32).prefix(16), radix: 16),
              let word3 = UInt64(hexadecimal.suffix(16), radix: 16)
        else { return nil }
        self.init(word0: word0, word1: word1, word2: word2, word3: word3)
    }

    var stableString: String {
        String(format: "%016llx%016llx%016llx%016llx", word0, word1, word2, word3)
    }
}

extension CommandId {
    init(uuid: UUID) {
        let parts = uuid.coreIdentifierParts
        self.init(high: parts.high, low: parts.low)
    }
}

extension CancellationId {
    init(uuid: UUID) {
        let parts = uuid.coreIdentifierParts
        self.init(high: parts.high, low: parts.low)
    }
}

extension HostRequestId {
    init(uuid: UUID) {
        let parts = uuid.coreIdentifierParts
        self.init(high: parts.high, low: parts.low)
    }
}

extension RecallQueryId {
    init(uuid: UUID) {
        let hexadecimal = uuid.uuidString.replacingOccurrences(of: "-", with: "")
        self.init(
            high: UInt64(hexadecimal.prefix(16), radix: 16)!,
            low: UInt64(hexadecimal.suffix(16), radix: 16)!
        )
    }

    var stableString: String { coreIdentifier(high: high, low: low) }
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

private extension UUID {
    var coreIdentifierParts: (high: UInt64, low: UInt64) {
        let hexadecimal = uuidString.replacingOccurrences(of: "-", with: "")
        return (
            UInt64(hexadecimal.prefix(16), radix: 16)!,
            UInt64(hexadecimal.suffix(16), radix: 16)!
        )
    }

    init?(coreHigh: UInt64, low: UInt64) {
        let hexadecimal = String(format: "%016llX%016llX", coreHigh, low)
        let formatted = [
            hexadecimal.prefix(8),
            hexadecimal.dropFirst(8).prefix(4),
            hexadecimal.dropFirst(12).prefix(4),
            hexadecimal.dropFirst(16).prefix(4),
            hexadecimal.dropFirst(20),
        ].map(String.init).joined(separator: "-")
        self.init(uuidString: formatted)
    }
}

private func coreIdentifier(high: UInt64, low: UInt64) -> String {
    String(format: "%016llx%016llx", high, low)
}
