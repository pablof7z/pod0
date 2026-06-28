import Foundation

// MARK: - Friends

extension AppStateStore {

    @discardableResult
    func addFriend(displayName: String, identifier: String) -> Friend {
        let friend = Friend(displayName: displayName, identifier: identifier)
        dispatchAddFriendToKernel(friend)
        // Bounded optimistic echo. The next kernel snapshot replaces
        // `state.friends` from `PodcastUpdate.friends`.
        state.friends.append(friend)
        return friend
    }

    func updateFriend(_ friend: Friend) {
        dispatchAddFriendToKernel(friend)
        guard let idx = state.friends.firstIndex(where: { $0.id == friend.id }) else { return }
        state.friends[idx] = friend
    }

    func updateFriendDisplayName(_ id: UUID, newName: String) {
        kernel?.dispatch(namespace: "podcast.social",
                         body: ["op": "update_friend_name",
                                "id": id.uuidString,
                                "display_name": newName])
        guard let idx = state.friends.firstIndex(where: { $0.id == id }) else { return }
        state.friends[idx].displayName = newName
    }

    func removeFriend(_ id: UUID) {
        guard let idx = state.friends.firstIndex(where: { $0.id == id }) else { return }
        kernel?.dispatch(namespace: "podcast.social",
                         body: ["op": "remove_friend", "id": id.uuidString])
        state.friends.remove(at: idx)
    }

    func friend(withID id: UUID) -> Friend? {
        state.friends.first { $0.id == id }
    }

    static func friend(from summary: FriendSummary) -> Friend? {
        guard let id = UUID(uuidString: summary.id) else { return nil }
        var friend = Friend(displayName: summary.displayName, identifier: summary.pubkeyHex)
        friend.id = id
        friend.addedAt = Date(timeIntervalSince1970: TimeInterval(summary.addedAt))
        friend.avatarURL = summary.avatarUrl
        friend.about = summary.about
        return friend
    }

    private func dispatchAddFriendToKernel(_ friend: Friend) {
        var body: [String: Any] = [
            "op": "add_friend",
            "id": friend.id.uuidString,
            "display_name": friend.displayName,
            "pubkey_hex": friend.identifier,
            "added_at": Int(friend.addedAt.timeIntervalSince1970)
        ]
        if let avatarURL = friend.avatarURL {
            body["avatar_url"] = avatarURL
        }
        if let about = friend.about {
            body["about"] = about
        }
        kernel?.dispatch(namespace: "podcast.social", body: body)
    }
}
