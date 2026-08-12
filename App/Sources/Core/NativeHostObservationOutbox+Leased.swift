import Foundation
import Pod0Core

extension NativeHostObservationOutbox {
    @discardableResult
    func persistBeforeDelivery(_ envelope: LeasedHostObservationEnvelope) throws -> Bool {
        let stored = try NativeLeasedHostObservationArchive.store(envelope, limits: limits)
        if leasedEntries.contains(where: { $0.stored.envelopeBytes == stored.envelopeBytes }) {
            return false
        }
        if leasedEntries.contains(where: {
            NativeLeasedHostObservationArchive.identity($0.envelope)
                == NativeLeasedHostObservationArchive.identity(envelope)
        }) {
            throw OutboxError.conflictingObservationIdentity
        }
        guard legacyEntryCount + leasedEntries.count < limits.maximumRecordCount else {
            throw OutboxError.recordLimitExceeded
        }
        let updated = leasedEntries + [LeasedEntry(stored: stored, envelope: envelope)]
        try persistLeased(updated)
        leasedEntries = updated
        return true
    }

    func pendingLeasedObservations() -> [LeasedHostObservationEnvelope] {
        leasedEntries.map(\.envelope)
    }

    func beginDelivery(of envelope: LeasedHostObservationEnvelope) -> Bool {
        guard let index = leasedIndex(of: envelope) else { return true }
        let current = leasedEntries[index].stored.abandonedDeliveries
        let attempts = current == UInt32.max ? UInt32.max : current + 1
        var updated = leasedEntries
        updated[index].stored.abandonedDeliveries = attempts
        guard (try? persistLeased(updated)) != nil else { return true }
        leasedEntries = updated
        return true
    }

    func finishDelivery(of envelope: LeasedHostObservationEnvelope) {
        guard let index = leasedIndex(of: envelope),
              leasedEntries[index].stored.abandonedDeliveries != 0
        else { return }
        var updated = leasedEntries
        updated[index].stored.abandonedDeliveries = 0
        guard (try? persistLeased(updated)) != nil else { return }
        leasedEntries = updated
    }

    @discardableResult
    func acknowledgeLeased(_ receipt: HostObservationReceipt) throws -> Bool {
        guard let requestID = Self.terminalRequestID(receipt) else { return false }
        guard leasedEntries.contains(where: {
            $0.envelope.observation.requestId == requestID
        }) else { return false }
        let updated = leasedEntries.filter { $0.envelope.observation.requestId != requestID }
        try persistLeased(updated)
        leasedEntries = updated
        return true
    }

    private func leasedIndex(of envelope: LeasedHostObservationEnvelope) -> Int? {
        let identity = NativeLeasedHostObservationArchive.identity(envelope)
        return leasedEntries.firstIndex {
            NativeLeasedHostObservationArchive.identity($0.envelope) == identity
        }
    }

    private func persistLeased(_ updated: [LeasedEntry]) throws {
        guard try legacyEncodedSize() + NativeLeasedHostObservationArchive.encodedSize(updated)
            <= limits.maximumArchiveBytes
        else { throw OutboxError.archiveTooLarge }
        try NativeLeasedHostObservationArchive.write(updated, to: leasedFileURL, limits: limits)
    }
}
