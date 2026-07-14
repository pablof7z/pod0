import Foundation

// MARK: - Nostr Pending Approval
// A contact requesting communication before being explicitly allowed or blocked.

struct NostrPendingApproval: Codable, Identifiable, Hashable, Sendable {

    private enum Constants {
        static let pubkeyTruncationHalfLength = 8
    }
    var id: UUID
    var pubkeyHex: String
    var displayName: String?
    var about: String?
    var pictureURL: String?
    var receivedAt: Date
    /// Text content of the inbound event that triggered this approval —
    /// rendered in the trust-this-user sheet so the user has context for
    /// the Allow/Block decision. Optional for forward compat with older
    /// persisted approvals that predate this field.
    var content: String?

    init(
        pubkeyHex: String,
        displayName: String? = nil,
        about: String? = nil,
        pictureURL: String? = nil,
        content: String? = nil
    ) {
        self.id = UUID()
        self.pubkeyHex = pubkeyHex
        self.displayName = displayName
        self.about = about
        self.pictureURL = pictureURL
        self.receivedAt = Date()
        self.content = content
    }

    var shortPubkey: String {
        let half = Constants.pubkeyTruncationHalfLength
        guard pubkeyHex.count > half * 2 else { return pubkeyHex }
        return "\(pubkeyHex.prefix(half))…\(pubkeyHex.suffix(half))"
    }
}
