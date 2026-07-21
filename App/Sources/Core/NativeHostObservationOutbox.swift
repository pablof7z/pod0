import Foundation
import Pod0Core

/// Process-death-durable transport evidence waiting for Rust acknowledgement.
/// This actor never interprets or merges observations and is not product truth.
actor NativeHostObservationOutbox {
    struct Limits: Sendable, Equatable {
        static let standard = Limits(
            maximumRecordCount: 64,
            maximumEnvelopeBytes: 2 * 1_024 * 1_024,
            maximumArchiveBytes: 16 * 1_024 * 1_024
        )

        let maximumRecordCount: Int
        let maximumEnvelopeBytes: Int
        let maximumArchiveBytes: Int
    }

    enum OutboxError: Error, Equatable {
        case invalidLimits
        case recordLimitExceeded
        case conflictingObservationIdentity
        case envelopeTooLarge
        case archiveTooLarge
        case unsupportedSchema
        case invalidArchive
        case receiptRequestMismatch
    }

    typealias Delivery = @Sendable (HostObservationEnvelope) async -> HostObservationReceipt

    private struct Archive: Codable {
        let schemaVersion: UInt32
        let records: [StoredRecord]
    }

    private struct StoredRecord: Codable, Equatable, Sendable {
        let requestHigh: UInt64
        let requestLow: UInt64
        let sequenceNumber: UInt64
        let envelopeBytes: Data
    }

    private struct Entry: Sendable {
        let stored: StoredRecord
        let envelope: HostObservationEnvelope
    }

    private static let schemaVersion: UInt32 = 1
    private let fileURL: URL
    private let limits: Limits
    private var entries: [Entry]
    private var isDelivering = false

    init(
        fileURL: URL? = nil,
        limits: Limits = .standard,
        fileManager: FileManager = .default
    ) throws {
        guard limits.maximumRecordCount > 0,
              limits.maximumEnvelopeBytes > 0,
              limits.maximumArchiveBytes > 0
        else { throw OutboxError.invalidLimits }
        self.fileURL = try fileURL ?? Self.defaultFileURL(fileManager: fileManager)
        self.limits = limits
        entries = try Self.restore(from: self.fileURL, limits: limits)
    }

    /// Atomically persists exact generated evidence before the caller delivers it.
    /// Exact duplicate envelopes are idempotent.
    @discardableResult
    func persistBeforeDelivery(_ envelope: HostObservationEnvelope) throws -> Bool {
        let stored = try Self.store(envelope, limits: limits)
        guard !entries.contains(where: { $0.stored.envelopeBytes == stored.envelopeBytes }) else {
            return false
        }
        guard !entries.contains(where: {
            $0.envelope.requestId == envelope.requestId
                && $0.envelope.sequenceNumber == envelope.sequenceNumber
        }) else { throw OutboxError.conflictingObservationIdentity }
        guard entries.count < limits.maximumRecordCount else {
            throw OutboxError.recordLimitExceeded
        }
        let updated = entries + [Entry(stored: stored, envelope: envelope)]
        try persist(updated)
        entries = updated
        return true
    }

    /// Returns durable evidence in generation order, including records restored at launch.
    func pendingObservations() -> [HostObservationEnvelope] {
        entries.map(\.envelope)
    }

    func pendingCount() -> Int {
        entries.count
    }

    /// The integration path for newly generated evidence: persist first, then deliver.
    @discardableResult
    func persistAndDeliver(
        _ envelope: HostObservationEnvelope,
        using delivery: @escaping Delivery
    ) async throws -> HostObservationReceipt {
        _ = try persistBeforeDelivery(envelope)
        let receipt = await delivery(envelope)
        guard Self.requestID(receipt) == envelope.requestId else {
            throw OutboxError.receiptRequestMismatch
        }
        _ = try acknowledge(receipt)
        return receipt
    }

    /// Retires all evidence for a request only when Rust returns a terminal receipt.
    @discardableResult
    func acknowledge(_ receipt: HostObservationReceipt) throws -> Bool {
        guard let requestID = Self.terminalRequestID(receipt) else { return false }
        guard entries.contains(where: { $0.envelope.requestId == requestID }) else { return false }
        let updated = entries.filter { $0.envelope.requestId != requestID }
        try persist(updated)
        entries = updated
        return true
    }

    /// Delivers one launch snapshot. Nonterminal evidence stays durable for relaunch replay.
    @discardableResult
    func deliverPending(using delivery: @escaping Delivery) async throws -> Int {
        guard !isDelivering else { return 0 }
        isDelivering = true
        defer { isDelivering = false }
        let snapshot = entries
        var delivered = 0
        for entry in snapshot {
            guard entries.contains(where: { $0.stored == entry.stored }) else { continue }
            let receipt = await delivery(entry.envelope)
            guard Self.requestID(receipt) == entry.envelope.requestId else {
                throw OutboxError.receiptRequestMismatch
            }
            delivered += 1
            _ = try acknowledge(receipt)
        }
        return delivered
    }

    private func persist(_ updated: [Entry]) throws {
        let archive = Archive(
            schemaVersion: Self.schemaVersion,
            records: updated.map(\.stored)
        )
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let data = try encoder.encode(archive)
        guard data.count <= limits.maximumArchiveBytes else {
            throw OutboxError.archiveTooLarge
        }
        try FileManager.default.createDirectory(
            at: fileURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try data.write(to: fileURL, options: .atomic)
    }

    private static func restore(from url: URL, limits: Limits) throws -> [Entry] {
        guard FileManager.default.fileExists(atPath: url.path) else { return [] }
        let fileSize = try url.resourceValues(forKeys: [.fileSizeKey]).fileSize
        guard let fileSize, fileSize <= limits.maximumArchiveBytes else {
            throw OutboxError.archiveTooLarge
        }
        let data = try Data(contentsOf: url, options: .mappedIfSafe)
        guard data.count <= limits.maximumArchiveBytes else { throw OutboxError.archiveTooLarge }
        let archive: Archive
        do {
            archive = try JSONDecoder().decode(Archive.self, from: data)
        } catch {
            throw OutboxError.invalidArchive
        }
        guard archive.schemaVersion == schemaVersion else { throw OutboxError.unsupportedSchema }
        guard archive.records.count <= limits.maximumRecordCount else {
            throw OutboxError.recordLimitExceeded
        }
        var seen = Set<Data>()
        var identities = Set<ObservationIdentity>()
        return try archive.records.map { stored in
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

    private struct ObservationIdentity: Hashable {
        let requestID: HostRequestId
        let sequenceNumber: UInt64

        init(_ envelope: HostObservationEnvelope) {
            requestID = envelope.requestId
            sequenceNumber = envelope.sequenceNumber
        }
    }

    private static func store(
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

    private static func requestID(_ receipt: HostObservationReceipt) -> HostRequestId {
        switch receipt {
        case .acceptedTransient(let requestID), .retainAndRetry(let requestID): requestID
        case .persisted(let requestID, _), .rejected(let requestID, _): requestID
        }
    }

    private static func terminalRequestID(_ receipt: HostObservationReceipt) -> HostRequestId? {
        switch receipt {
        case .persisted(let requestID, terminal: true), .rejected(let requestID, _): requestID
        case .acceptedTransient, .persisted, .retainAndRetry: nil
        }
    }

    private static func defaultFileURL(fileManager: FileManager) throws -> URL {
        try fileManager.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        )
        .appendingPathComponent("podcastr", isDirectory: true)
        .appendingPathComponent("native-host-observation-outbox-v1.json")
    }

}
