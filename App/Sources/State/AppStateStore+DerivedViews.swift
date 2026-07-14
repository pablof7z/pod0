import Foundation

// MARK: - Derived Views

extension AppStateStore {

    var activeNotes: [Note] {
        state.notes.filter { !$0.deleted }
    }

    // MARK: - Friend-scoped helpers

    /// All friends sorted alphabetically by display name — the canonical order
    /// for any list that shows friends without an explicit user-defined sort.
    var sortedFriends: [Friend] {
        state.friends.sorted {
            $0.displayName.localizedCaseInsensitiveCompare($1.displayName) == .orderedAscending
        }
    }

    /// Returns the live friend record matching `id`, or `nil` when not found.
    func friend(id: UUID) -> Friend? {
        state.friends.first { $0.id == id }
    }

    /// Returns the live friend record whose Nostr identifier (pubkey hex or npub)
    /// matches `identifier`, or `nil` when not found.
    func friend(identifier: String) -> Friend? {
        state.friends.first { $0.identifier == identifier }
    }

    /// Most recent note targeting the friend, used to sort the friends list.
    /// Returns `nil` when no notes target this friend.
    func lastActivity(forFriend friendID: UUID) -> Date? {
        state.notes
            .filter { note -> Bool in
                guard !note.deleted, let target = note.target else { return false }
                if case .friend(let id) = target, id == friendID { return true }
                return false
            }
            .map(\.createdAt)
            .max()
    }
}
