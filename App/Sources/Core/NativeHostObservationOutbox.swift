import Foundation
import Pod0Core

/// Process-death-durable transport evidence waiting for Rust acknowledgement.
/// This actor never interprets or merges observations and is not product truth.
actor NativeHostObservationOutbox {
    struct Limits: Sendable, Equatable {
        static let standard = Limits(
            maximumRecordCount: 64,
            maximumEnvelopeBytes: 40 * 1_024 * 1_024,
            maximumArchiveBytes: 128 * 1_024 * 1_024
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

    private typealias Entry = NativeHostObservationArchive.Entry
    typealias LeasedEntry = NativeLeasedHostObservationArchive.Entry

    private let fileURL: URL
    let leasedFileURL: URL
    let limits: Limits
    private var entries: [Entry]
    var leasedEntries: [LeasedEntry]
    private var isDelivering = false
    var isDeliveringLeased = false

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
        leasedFileURL = Self.leasedFileURL(for: self.fileURL)
        self.limits = limits
        entries = try NativeHostObservationArchive.restore(from: self.fileURL, limits: limits)
        leasedEntries = try NativeLeasedHostObservationArchive.restore(
            from: leasedFileURL,
            limits: limits
        )
        guard entries.count + leasedEntries.count <= limits.maximumRecordCount else {
            throw OutboxError.recordLimitExceeded
        }
        guard try NativeHostObservationArchive.encodedSize(entries)
            + NativeLeasedHostObservationArchive.encodedSize(leasedEntries)
            <= limits.maximumArchiveBytes
        else { throw OutboxError.archiveTooLarge }
    }

    /// Atomically persists exact generated evidence before the caller delivers it.
    /// Exact duplicate envelopes are idempotent.
    @discardableResult
    func persistBeforeDelivery(_ envelope: HostObservationEnvelope) throws -> Bool {
        let stored = try NativeHostObservationArchive.store(envelope, limits: limits)
        guard !entries.contains(where: { $0.stored.envelopeBytes == stored.envelopeBytes }) else {
            return false
        }
        guard !entries.contains(where: {
            $0.envelope.requestId == envelope.requestId
                && $0.envelope.sequenceNumber == envelope.sequenceNumber
        }) else { throw OutboxError.conflictingObservationIdentity }
        guard entries.count + leasedEntries.count < limits.maximumRecordCount else {
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

    /// Records that a delivery is about to cross into Rust, so a process death
    /// inside that call is still visible after relaunch. Durable evidence is
    /// never discarded merely because delivery repeatedly abandoned it.
    func beginDelivery(of envelope: HostObservationEnvelope) -> Bool {
        guard let index = entries.firstIndex(where: { Self.matches($0, envelope) }) else {
            return true
        }
        let current = entries[index].stored.abandonedDeliveries
        let attempts = current == UInt32.max ? UInt32.max : current + 1
        var updated = entries
        updated[index] = Self.marking(updated[index], abandonedDeliveries: attempts)
        guard (try? persist(updated)) != nil else { return true }
        entries = updated
        return true
    }

    /// Clears the in-flight mark once Rust returns any receipt at all.
    func finishDelivery(of envelope: HostObservationEnvelope) {
        guard let index = entries.firstIndex(where: { Self.matches($0, envelope) }),
              entries[index].stored.abandonedDeliveries != 0
        else { return }
        var updated = entries
        updated[index] = Self.marking(updated[index], abandonedDeliveries: 0)
        guard (try? persist(updated)) != nil else { return }
        entries = updated
    }

    private static func matches(_ entry: Entry, _ envelope: HostObservationEnvelope) -> Bool {
        entry.envelope.requestId == envelope.requestId
            && entry.envelope.sequenceNumber == envelope.sequenceNumber
    }

    private static func marking(_ entry: Entry, abandonedDeliveries: UInt32) -> Entry {
        var stored = entry.stored
        stored.abandonedDeliveries = abandonedDeliveries
        return Entry(stored: stored, envelope: entry.envelope)
    }

    func pendingCount() -> Int {
        entries.count + leasedEntries.count
    }

    var legacyEntryCount: Int { entries.count }

    func legacyEncodedSize() throws -> Int {
        try NativeHostObservationArchive.encodedSize(entries)
    }

    /// The integration path for newly generated evidence: persist first, then deliver.
    @discardableResult
    func persistAndDeliver(
        _ envelope: HostObservationEnvelope,
        using delivery: @escaping Delivery
    ) async throws -> HostObservationReceipt {
        _ = try persistBeforeDelivery(envelope)
        guard beginDelivery(of: envelope) else {
            return .retainAndRetry(requestId: envelope.requestId)
        }
        let receipt = await delivery(envelope)
        finishDelivery(of: envelope)
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
            guard beginDelivery(of: entry.envelope) else { continue }
            let receipt = await delivery(entry.envelope)
            finishDelivery(of: entry.envelope)
            guard Self.requestID(receipt) == entry.envelope.requestId else {
                throw OutboxError.receiptRequestMismatch
            }
            delivered += 1
            _ = try acknowledge(receipt)
        }
        return delivered
    }

    private func persist(_ updated: [Entry]) throws {
        guard try NativeHostObservationArchive.encodedSize(updated)
            + NativeLeasedHostObservationArchive.encodedSize(leasedEntries)
            <= limits.maximumArchiveBytes
        else { throw OutboxError.archiveTooLarge }
        try NativeHostObservationArchive.write(updated, to: fileURL, limits: limits)
    }

    static func requestID(_ receipt: HostObservationReceipt) -> HostRequestId {
        switch receipt {
        case .acceptedTransient(let requestID), .retainAndRetry(let requestID): requestID
        case .persisted(let requestID, _), .rejected(let requestID, _): requestID
        }
    }

    static func terminalRequestID(_ receipt: HostObservationReceipt) -> HostRequestId? {
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

    private static func leasedFileURL(for legacyURL: URL) -> URL {
        legacyURL.deletingPathExtension()
            .appendingPathExtension("leased.json")
    }

}
