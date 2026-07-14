import Foundation

// MARK: - Peer-conversation tool surface
//
// Ported from win-the-day's `end_conversation` and `send_friend_message`. Both
// tools are peer-only: when the dispatch has no `peerContext` they early-return
// a clean `toolError`, since calling them outside a Nostr peer turn has no
// well-defined effect.

extension AgentTools {

    // MARK: - end_conversation

    static func endConversationTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let peerContext = deps.peerContext else {
            return toolError("end_conversation requires a peer conversation context")
        }
        guard let reason = (args["reason"] as? String)?.trimmed, !reason.isEmpty else {
            return toolError("Missing or empty 'reason'")
        }
        return toolSuccess([
            "no_reply": true,
            "reason": reason,
            "root_event_id": peerContext.rootEventID,
        ])
    }

    // MARK: - send_friend_message

    /// Publish a kind:1 note p-tagged at a named friend. Per the current
    /// design (mirroring win-the-day's `.peerAgent` channel), this tool is
    /// peer-only — calling it outside a peer conversation returns a clean
    /// tool error. Inside a peer turn, the reply is threaded under the
    /// active conversation root via NIP-10 tags.
    static func sendFriendMessageTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let input = (args["friend_pubkey"] as? String)?.trimmed, !input.isEmpty else {
            return toolError("Missing or empty 'friend_pubkey'")
        }
        guard let message = (args["message"] as? String)?.trimmed, !message.isEmpty else {
            return toolError("Missing or empty 'message'")
        }
        // Resolve prefix or full pubkey — also gates on the Friends list so
        // the agent cannot fire kind:1 events at arbitrary pubkeys.
        guard let friendPubkey = await deps.friendDirectory.resolvePubkey(prefixOrFull: input) else {
            return toolError("No friend found matching '\(input)'. Add them first.")
        }
        do {
            let eventID = try await deps.peerPublisher.publishFriendMessage(
                friendPubkeyHex: friendPubkey,
                body: message,
                peerContext: deps.peerContext
            )
            // Determine where to resume once the friend replies. The two
            // origins are mutually exclusive: an in-app chat has a
            // chatConversationID; a Nostr peer turn has a peerContext.
            let origin: PendingFriendMessageOrigin?
            if let convID = deps.chatConversationID {
                origin = .inAppChat(conversationID: convID)
            } else if let ctx = deps.peerContext {
                origin = .nostrPeer(rootEventID: ctx.rootEventID, peerPubkey: ctx.peerPubkeyHex)
            } else {
                origin = nil
            }
            if let origin {
                let pending = PendingFriendMessage(
                    sentEventID: eventID,
                    friendPubkey: friendPubkey,
                    sentAt: Date(),
                    origin: origin
                )
                await deps.pendingRegistrar?.register(pending)
            }
            var result: [String: Any] = [
                "event_id": eventID,
                "friend_pubkey": friendPubkey,
                "re_invocation": "Message sent. Once the other agent responds you will be automatically re-invoked in this conversation with their reply.",
            ]
            if let rootID = deps.peerContext?.rootEventID {
                result["root_event_id"] = rootID
            }
            return toolSuccess(result)
        } catch {
            return toolError("send_friend_message failed: \(error.localizedDescription)")
        }
    }
}
