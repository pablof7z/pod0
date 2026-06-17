import Foundation

// MARK: - Pending friend message registry

extension AppStateStore {

    private struct PendingFriendMessagesResponse: Decodable {
        let messages: [PendingFriendMessage]
        let claimed: PendingFriendMessage?
        let exists: Bool?
    }

    private static let pendingMessageMaxAgeSecs: TimeInterval = 7 * 24 * 60 * 60

    private static let pendingFriendMessageEncoder: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        return encoder
    }()

    private static let pendingFriendMessageDecoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }()

    /// Persists a pending friend message so the responder can route incoming
    /// replies back to the originating conversation. Idempotent: an existing
    /// entry with the same `sentEventID` is replaced. Triggers a TTL sweep.
    func registerPendingFriendMessage(_ message: PendingFriendMessage) {
        guard let response = pendingFriendPolicy(
            op: "pending_friend_register",
            extra: ["message": pendingFriendMessageObject(message)]
        ) else { return }
        state.pendingFriendMessages = response.messages
    }

    /// Removes and returns the pending entry whose `sentEventID` matches
    /// `rootEventID` (the NIP-10 root of the friend's inbound reply). Returns
    /// `nil` when no entry matches or the entry has expired.
    @discardableResult
    func claimPendingFriendMessage(forRootEventID rootEventID: String) -> PendingFriendMessage? {
        guard let response = pendingFriendPolicy(
            op: "pending_friend_claim",
            extra: ["root_event_id": rootEventID]
        ) else { return nil }
        state.pendingFriendMessages = response.messages
        return response.claimed
    }

    /// Returns `true` iff any non-expired pending friend message exists with
    /// `sentEventID == rootEventID`. Does NOT remove the entry.
    func hasPendingFriendMessage(forRootEventID rootEventID: String) -> Bool {
        guard let response = pendingFriendPolicy(
            op: "pending_friend_has",
            extra: ["root_event_id": rootEventID]
        ) else { return false }
        state.pendingFriendMessages = response.messages
        return response.exists ?? false
    }

    private func pendingFriendPolicy(
        op: String,
        extra: [String: Any] = [:]
    ) -> PendingFriendMessagesResponse? {
        var payload: [String: Any] = [
            "op": op,
            "messages": pendingFriendMessageObjects(state.pendingFriendMessages),
            "cutoff": Self.pendingFriendMessageISO8601(
                Date().addingTimeInterval(-Self.pendingMessageMaxAgeSecs)
            ),
        ]
        for (key, value) in extra {
            payload[key] = value
        }
        guard let data = try? JSONSerialization.data(withJSONObject: payload),
              let json = String(data: data, encoding: .utf8)
        else { return nil }
        return json.withCString { ptr -> PendingFriendMessagesResponse? in
            guard let result = nmp_app_podcast_agent_action_policy(ptr) else {
                return nil
            }
            defer { nmp_free_string(result) }
            let response = String(cString: result)
            guard let data = response.data(using: .utf8)
            else { return nil }
            return try? Self.pendingFriendMessageDecoder.decode(
                PendingFriendMessagesResponse.self,
                from: data
            )
        }
    }

    private func pendingFriendMessageObjects(_ messages: [PendingFriendMessage]) -> [[String: Any]] {
        guard let data = try? Self.pendingFriendMessageEncoder.encode(messages),
              let object = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]]
        else { return [] }
        return object
    }

    private func pendingFriendMessageObject(_ message: PendingFriendMessage) -> [String: Any] {
        guard let data = try? Self.pendingFriendMessageEncoder.encode(message),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return [:] }
        return object
    }

    private static func pendingFriendMessageISO8601(_ date: Date) -> String {
        guard let data = try? pendingFriendMessageEncoder.encode(["date": date]),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: String],
              let encoded = object["date"]
        else { return "" }
        return encoded
    }
}
