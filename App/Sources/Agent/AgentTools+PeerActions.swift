import Foundation

// MARK: - Peer-conversation tool surface
//
// Ported from win-the-day's `end_conversation` and `send_friend_message`. Both
// tools are peer-only: when the dispatch has no `peerContext` they early-return
// a clean `toolError`, since calling them outside a Nostr peer turn has no
// well-defined effect.

extension AgentTools {
    private struct PeerEndPlan: Decodable {
        let error: String?
        let reason: String?
        let rootEventID: String?

        enum CodingKeys: String, CodingKey {
            case error, reason
            case rootEventID = "root_event_id"
        }
    }

    private struct PeerMessagePlan: Decodable {
        let error: String?
        let friendPubkey: String?
        let message: String?

        enum CodingKeys: String, CodingKey {
            case error, message
            case friendPubkey = "friend_pubkey"
        }
    }

    // MARK: - end_conversation

    static func endConversationTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        var payload = args
        if let rootEventID = deps.peerContext?.rootEventID { payload["root_event_id"] = rootEventID }
        guard let plan = await peerActionPlan(PeerEndPlan.self, op: "peer_end_plan", payload: payload) else {
            return toolError("end_conversation planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        guard let reason = plan.reason, let rootEventID = plan.rootEventID else {
            return toolError("end_conversation plan was incomplete")
        }
        return await actionTool(
            op: "peer_end_result",
            payload: ["reason": reason, "root_event_id": rootEventID]
        ) ?? toolError("end_conversation result shaping is unavailable")
    }

    // MARK: - send_friend_message

    /// Publish a kind:1 note p-tagged at a named friend. Per the current
    /// design (mirroring win-the-day's `.peerAgent` channel), this tool is
    /// peer-only — calling it outside a peer conversation returns a clean
    /// tool error. Inside a peer turn, the reply is threaded under the
    /// active conversation root via NIP-10 tags.
    static func sendFriendMessageTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let plan = await peerActionPlan(PeerMessagePlan.self, op: "peer_message_plan", payload: args) else {
            return toolError("send_friend_message planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        guard let input = plan.friendPubkey, let message = plan.message else {
            return toolError("send_friend_message plan was incomplete")
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
            ]
            if let rootID = deps.peerContext?.rootEventID {
                result["root_event_id"] = rootID
            }
            return await actionTool(op: "peer_message_result", payload: result)
                ?? toolError("send_friend_message result shaping is unavailable")
        } catch {
            return toolError("send_friend_message failed: \(error.localizedDescription)")
        }
    }

    private static func peerActionPlan<T: Decodable>(
        _ type: T.Type,
        op: String,
        payload: [String: Any]
    ) async -> T? {
        guard let envelope = await actionTool(op: op, payload: payload),
              let data = envelope.data(using: .utf8)
        else { return nil }
        return try? JSONDecoder().decode(T.self, from: data)
    }
}
