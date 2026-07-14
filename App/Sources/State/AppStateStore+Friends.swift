import Foundation

// MARK: - Friends

extension AppStateStore {

    @discardableResult
    func addFriend(displayName: String, identifier: String) -> Friend {
        let friend = Friend(displayName: displayName, identifier: identifier)
        state.friends.append(friend)
        state.nostrAllowedPubkeys.insert(identifier)
        return friend
    }

    func updateFriend(_ friend: Friend) {
        guard let idx = state.friends.firstIndex(where: { $0.id == friend.id }) else { return }
        state.friends[idx] = friend
    }

    func updateFriendDisplayName(_ id: UUID, newName: String) {
        guard let idx = state.friends.firstIndex(where: { $0.id == id }) else { return }
        state.friends[idx].displayName = newName
    }

    func removeFriend(_ id: UUID) {
        guard let idx = state.friends.firstIndex(where: { $0.id == id }) else { return }
        let identifier = state.friends[idx].identifier
        state.friends.remove(at: idx)
        state.nostrAllowedPubkeys.remove(identifier)
    }

    func friend(withID id: UUID) -> Friend? {
        state.friends.first { $0.id == id }
    }
}
