import Foundation
import Pod0Core

/// On-disk representation of the host-observation outbox.
///
/// Split from `NativeHostObservationOutbox` so the actor owns only in-memory
/// state and delivery fencing while this type owns encoding, bounds, and
/// tamper checks. It never interprets an observation.
enum NativeHostObservationArchive {

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
        let envelopeBytes: Data
        /// Deliveries begun but never returned from — i.e. the process died
        /// inside the FFI call. Cleared on every returned receipt, so ordinary
        /// `retainAndRetry` churn never counts against a record.
        var abandonedDeliveries: UInt32 = 0

        init(
            requestHigh: UInt64,
            requestLow: UInt64,
            sequenceNumber: UInt64,
            envelopeBytes: Data,
            abandonedDeliveries: UInt32 = 0
        ) {
            self.requestHigh = requestHigh
            self.requestLow = requestLow
            self.sequenceNumber = sequenceNumber
            self.envelopeBytes = envelopeBytes
            self.abandonedDeliveries = abandonedDeliveries
        }

        /// Archives written before delivery fencing existed carry no counter.
        init(from decoder: any Decoder) throws {
            let container = try decoder.container(keyedBy: CodingKeys.self)
            requestHigh = try container.decode(UInt64.self, forKey: .requestHigh)
            requestLow = try container.decode(UInt64.self, forKey: .requestLow)
            sequenceNumber = try container.decode(UInt64.self, forKey: .sequenceNumber)
            envelopeBytes = try container.decode(Data.self, forKey: .envelopeBytes)
            abandonedDeliveries =
                try container.decodeIfPresent(UInt32.self, forKey: .abandonedDeliveries) ?? 0
        }
    }

    struct Entry: Sendable {
        let stored: StoredRecord
        let envelope: HostObservationEnvelope
    }

    private struct ObservationIdentity: Hashable {
        let requestID: HostRequestId
        let sequenceNumber: UInt64

        init(_ envelope: HostObservationEnvelope) {
            requestID = envelope.requestId
            sequenceNumber = envelope.sequenceNumber
        }
    }

    static func write(_ entries: [Entry], to url: URL, limits: Limits) throws {
        let payload = Payload(
            schemaVersion: schemaVersion,
            records: entries.map(\.stored)
        )
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let data = try encoder.encode(payload)
        guard data.count <= limits.maximumArchiveBytes else {
            throw OutboxError.archiveTooLarge
        }
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try data.write(to: url, options: .atomic)
    }

    static func restore(from url: URL, limits: Limits) throws -> [Entry] {
        guard FileManager.default.fileExists(atPath: url.path) else { return [] }
        let fileSize = try url.resourceValues(forKeys: [.fileSizeKey]).fileSize
        guard let fileSize, fileSize <= limits.maximumArchiveBytes else {
            throw OutboxError.archiveTooLarge
        }
        let data = try Data(contentsOf: url, options: .mappedIfSafe)
        guard data.count <= limits.maximumArchiveBytes else { throw OutboxError.archiveTooLarge }
        let payload: Payload
        do {
            payload = try JSONDecoder().decode(Payload.self, from: data)
        } catch {
            throw OutboxError.invalidArchive
        }
        guard payload.schemaVersion == schemaVersion else { throw OutboxError.unsupportedSchema }
        guard payload.records.count <= limits.maximumRecordCount else {
            throw OutboxError.recordLimitExceeded
        }
        var seen = Set<Data>()
        var identities = Set<ObservationIdentity>()
        return try payload.records.map { stored in
            guard stored.envelopeBytes.count <= limits.maximumEnvelopeBytes,
                  seen.insert(stored.envelopeBytes).inserted
            else { throw OutboxError.invalidArchive }
            let envelope = try decode(stored.envelopeBytes)
            guard envelope.requestId.high == stored.requestHigh,
                  envelope.requestId.low == stored.requestLow,
                  envelope.sequenceNumber == stored.sequenceNumber
            else { throw OutboxError.invalidArchive }
            guard identities.insert(ObservationIdentity(envelope)).inserted else {
                throw OutboxError.invalidArchive
            }
            return Entry(stored: stored, envelope: envelope)
        }
    }

    static func store(
        _ envelope: HostObservationEnvelope,
        limits: Limits
    ) throws -> StoredRecord {
        var bytes: [UInt8] = []
        FfiConverterTypeHostObservationEnvelope.write(envelope, into: &bytes)
        guard bytes.count <= limits.maximumEnvelopeBytes else {
            throw OutboxError.envelopeTooLarge
        }
        return StoredRecord(
            requestHigh: envelope.requestId.high,
            requestLow: envelope.requestId.low,
            sequenceNumber: envelope.sequenceNumber,
            envelopeBytes: Data(bytes)
        )
    }

    private static func decode(_ data: Data) throws -> HostObservationEnvelope {
        var buffer = (data: data, offset: data.startIndex)
        do {
            let envelope = try FfiConverterTypeHostObservationEnvelope.read(from: &buffer)
            guard buffer.offset == data.endIndex else { throw OutboxError.invalidArchive }
            return envelope
        } catch let error as OutboxError {
            throw error
        } catch {
            throw OutboxError.invalidArchive
        }
    }
}
