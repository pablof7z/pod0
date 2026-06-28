import Foundation

// MARK: - One-shot social native-store migration

struct SocialNativeStoreMigration {
    struct Payload {
        let notes: [Note]
        let friends: [Friend]
    }

    struct Command {
        let namespace: String
        let body: [String: Any]
    }

    static let flagKey = "socialNativeStoreMigrationV1"

    static func pendingPayload(from state: AppState, defaults: UserDefaults = .standard) -> Payload? {
        guard !defaults.bool(forKey: flagKey) else { return nil }
        return Payload(notes: state.notes, friends: state.friends)
    }

    static func commands(from payload: Payload) -> [Command] {
        var commands: [Command] = []
        commands.reserveCapacity(payload.notes.count * 2 + payload.friends.count)

        for note in payload.notes {
            commands.append(Command(namespace: "podcast.social", body: addNoteBody(note)))
            if note.deleted {
                commands.append(Command(namespace: "podcast.social", body: [
                    "op": "delete_note",
                    "id": note.id.uuidString,
                ]))
            }
        }

        for friend in payload.friends {
            commands.append(Command(namespace: "podcast.social", body: addFriendBody(friend)))
        }

        return commands
    }

    static func markComplete(defaults: UserDefaults = .standard) {
        defaults.set(true, forKey: flagKey)
    }

    private static func addNoteBody(_ note: Note) -> [String: Any] {
        var body: [String: Any] = [
            "op": "add_note",
            "id": note.id.uuidString,
            "text": note.text,
            "kind": note.kind.rawValue,
            "created_at": Int(note.createdAt.timeIntervalSince1970),
            "author": note.author.rawValue,
        ]
        if let target = targetBody(from: note.target) {
            body["target"] = target
        }
        return body
    }

    private static func addFriendBody(_ friend: Friend) -> [String: Any] {
        var body: [String: Any] = [
            "op": "add_friend",
            "id": friend.id.uuidString,
            "display_name": friend.displayName,
            "pubkey_hex": friend.identifier,
            "added_at": Int(friend.addedAt.timeIntervalSince1970),
        ]
        if let avatarURL = friend.avatarURL {
            body["avatar_url"] = avatarURL
        }
        if let about = friend.about {
            body["about"] = about
        }
        return body
    }

    private static func targetBody(from anchor: Anchor?) -> [String: Any]? {
        guard let anchor else { return nil }
        switch anchor {
        case .note(let id):
            return ["type": "note", "note_id": id.uuidString]
        case .friend(let id):
            return ["type": "friend", "friend_id": id.uuidString]
        case .episode(let id, let positionSeconds):
            return [
                "type": "episode",
                "episode_id": id.uuidString,
                "position_secs": positionSeconds,
            ]
        }
    }
}
