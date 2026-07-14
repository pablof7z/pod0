import Foundation

// MARK: - Pending friend message registry

extension AppStateStore {

    private static let pendingMessageTTL: TimeInterval = 7 * 24 * 60 * 60

    /// Persists a pending friend message so the responder can route incoming
    /// replies back to the originating conversation. Idempotent: an existing
    /// entry with the same `sentEventID` is replaced. Triggers a TTL sweep.
    func registerPendingFriendMessage(_ message: PendingFriendMessage) {
        sweepExpiredPendingFriendMessages()
        state.pendingFriendMessages.removeAll { $0.sentEventID == message.sentEventID }
        state.pendingFriendMessages.append(message)
    }

    /// Removes and returns the pending entry whose `sentEventID` matches
    /// `rootEventID` (the NIP-10 root of the friend's inbound reply). Returns
    /// `nil` when no entry matches or the entry has expired.
    @discardableResult
    func claimPendingFriendMessage(forRootEventID rootEventID: String) -> PendingFriendMessage? {
        sweepExpiredPendingFriendMessages()
        guard let idx = state.pendingFriendMessages.firstIndex(where: {
            $0.sentEventID == rootEventID
        }) else { return nil }
        return state.pendingFriendMessages.remove(at: idx)
    }

    /// Returns `true` iff any non-expired pending friend message exists with
    /// `sentEventID == rootEventID`. Does NOT remove the entry.
    func hasPendingFriendMessage(forRootEventID rootEventID: String) -> Bool {
        state.pendingFriendMessages.contains { $0.sentEventID == rootEventID }
    }

    private func sweepExpiredPendingFriendMessages() {
        let cutoff = Date().addingTimeInterval(-Self.pendingMessageTTL)
        state.pendingFriendMessages.removeAll { $0.sentAt < cutoff }
    }
}
