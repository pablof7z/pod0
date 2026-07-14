import Foundation

// MARK: - PendingFriendMessage
//
// Tracks an outbound `send_friend_message` event that the agent has published
// and is waiting for a response to. When the friend's reply arrives, the
// responder claims the record and re-invokes the originating conversation.

struct PendingFriendMessage: Codable, Identifiable, Sendable {
    var id: String { sentEventID }
    /// Nostr event id the agent published (the root of the friend sub-thread).
    let sentEventID: String
    /// Full hex pubkey of the friend who was messaged.
    let friendPubkey: String
    let sentAt: Date
    /// Where the agent should resume once the friend replies.
    let origin: PendingFriendMessageOrigin
}

enum PendingFriendMessageOrigin: Codable, Sendable {
    case inAppChat(conversationID: UUID)
    case nostrPeer(rootEventID: String, peerPubkey: String)
}
