import Foundation
import Pod0Core

/// Exact on-disk transport evidence for observations tied to Rust effect leases.
/// Product truth remains the Rust journal; this archive only survives the gap
/// between native capability completion and Rust acknowledgement.
enum NativeLeasedHostObservationArchive {
    typealias Limits = NativeHostObservationOutbox.Limits
    typealias OutboxError = NativeHostObservationOutbox.OutboxError

    static let schemaVersion: UInt32 = 1

    struct Payload: Codable {
        let schemaVersion: UInt32
        let records: [StoredRecord]
    }

    struct StoredRecord: Codable, Equatable, Sendable {
        let requestHigh: UInt64
        let requestLow: UInt64
        let sequenceNumber: UInt64
        let intentHigh: UInt64
        let intentLow: UInt64
        let attemptHigh: UInt64
        let attemptLow: UInt64
        let fence: UInt64
        let envelopeBytes: Data
        var abandonedDeliveries: UInt32 = 0

        init(from decoder: any Decoder) throws {
            let values = try decoder.container(keyedBy: CodingKeys.self)
            requestHigh = try values.decode(UInt64.self, forKey: .requestHigh)
            requestLow = try values.decode(UInt64.self, forKey: .requestLow)
            sequenceNumber = try values.decode(UInt64.self, forKey: .sequenceNumber)
            intentHigh = try values.decode(UInt64.self, forKey: .intentHigh)
            intentLow = try values.decode(UInt64.self, forKey: .intentLow)
            attemptHigh = try values.decode(UInt64.self, forKey: .attemptHigh)
            attemptLow = try values.decode(UInt64.self, forKey: .attemptLow)
            fence = try values.decode(UInt64.self, forKey: .fence)
            envelopeBytes = try values.decode(Data.self, forKey: .envelopeBytes)
            abandonedDeliveries =
                try values.decodeIfPresent(UInt32.self, forKey: .abandonedDeliveries) ?? 0
        }

        init(_ envelope: LeasedHostObservationEnvelope, bytes: Data) {
            requestHigh = envelope.observation.requestId.high
            requestLow = envelope.observation.requestId.low
            sequenceNumber = envelope.observation.sequenceNumber
            intentHigh = envelope.lease.intentId.high
            intentLow = envelope.lease.intentId.low
            attemptHigh = envelope.lease.attemptId.high
            attemptLow = envelope.lease.attemptId.low
            fence = envelope.lease.fence
            envelopeBytes = bytes
        }
    }

    struct Entry: Sendable {
        var stored: StoredRecord
        let envelope: LeasedHostObservationEnvelope
    }

    static func write(_ entries: [Entry], to url: URL, limits: Limits) throws {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let data = try encoder.encode(Payload(
            schemaVersion: schemaVersion,
            records: entries.map(\.stored)
        ))
        guard data.count <= limits.maximumArchiveBytes else {
            throw OutboxError.archiveTooLarge
        }
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try data.write(to: url, options: .atomic)
    }

    static func encodedSize(_ entries: [Entry]) throws -> Int {
        try JSONEncoder().encode(Payload(
            schemaVersion: schemaVersion,
            records: entries.map(\.stored)
        )).count
    }

    static func restore(from url: URL, limits: Limits) throws -> [Entry] {
        guard FileManager.default.fileExists(atPath: url.path) else { return [] }
        let data = try Data(contentsOf: url, options: .mappedIfSafe)
        guard data.count <= limits.maximumArchiveBytes else { throw OutboxError.archiveTooLarge }
        let payload: Payload
        do { payload = try JSONDecoder().decode(Payload.self, from: data) }
        catch { throw OutboxError.invalidArchive }
        guard payload.schemaVersion == schemaVersion else { throw OutboxError.unsupportedSchema }
        guard payload.records.count <= limits.maximumRecordCount else {
            throw OutboxError.recordLimitExceeded
        }
        var bytesSeen = Set<Data>()
        var identities = Set<String>()
        return try payload.records.map { stored in
            guard stored.envelopeBytes.count <= limits.maximumEnvelopeBytes,
                  bytesSeen.insert(stored.envelopeBytes).inserted
            else { throw OutboxError.invalidArchive }
            let envelope = try decode(stored.envelopeBytes)
            guard matches(stored, envelope),
                  identities.insert(identity(envelope)).inserted
            else { throw OutboxError.invalidArchive }
            return Entry(stored: stored, envelope: envelope)
        }
    }

    static func store(
        _ envelope: LeasedHostObservationEnvelope,
        limits: Limits
    ) throws -> StoredRecord {
        var bytes: [UInt8] = []
        FfiConverterTypeLeasedHostObservationEnvelope.write(envelope, into: &bytes)
        guard bytes.count <= limits.maximumEnvelopeBytes else {
            throw OutboxError.envelopeTooLarge
        }
        return StoredRecord(envelope, bytes: Data(bytes))
    }

    static func identity(_ envelope: LeasedHostObservationEnvelope) -> String {
        let request = envelope.observation.requestId
        let intent = envelope.lease.intentId
        let attempt = envelope.lease.attemptId
        return "\(request.high):\(request.low):\(envelope.observation.sequenceNumber):"
            + "\(intent.high):\(intent.low):\(attempt.high):\(attempt.low):\(envelope.lease.fence)"
    }

    private static func matches(
        _ stored: StoredRecord,
        _ envelope: LeasedHostObservationEnvelope
    ) -> Bool {
        stored.requestHigh == envelope.observation.requestId.high
            && stored.requestLow == envelope.observation.requestId.low
            && stored.sequenceNumber == envelope.observation.sequenceNumber
            && stored.intentHigh == envelope.lease.intentId.high
            && stored.intentLow == envelope.lease.intentId.low
            && stored.attemptHigh == envelope.lease.attemptId.high
            && stored.attemptLow == envelope.lease.attemptId.low
            && stored.fence == envelope.lease.fence
    }

    private static func decode(_ data: Data) throws -> LeasedHostObservationEnvelope {
        var buffer = (data: data, offset: data.startIndex)
        do {
            let value = try FfiConverterTypeLeasedHostObservationEnvelope.read(from: &buffer)
            guard buffer.offset == data.endIndex else { throw OutboxError.invalidArchive }
            return value
        } catch let error as OutboxError { throw error }
        catch { throw OutboxError.invalidArchive }
    }
}
