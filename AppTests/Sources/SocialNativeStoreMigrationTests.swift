import Foundation
import XCTest
@testable import Podcastr

final class SocialNativeStoreMigrationTests: XCTestCase {
    func testPendingPayloadIsNilAfterCompletionFlag() {
        let suiteName = "SocialNativeStoreMigrationTests-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer { defaults.removePersistentDomain(forName: suiteName) }

        var state = AppState()
        state.notes = [Note(text: "legacy")]
        XCTAssertNotNil(SocialNativeStoreMigration.pendingPayload(from: state, defaults: defaults))

        defaults.set(true, forKey: SocialNativeStoreMigration.flagKey)
        XCTAssertNil(SocialNativeStoreMigration.pendingPayload(from: state, defaults: defaults))
    }

    func testCommandsSeedNotesThenDeletedTombstonesAndFriends() throws {
        let noteID = UUID(uuidString: "11111111-1111-1111-1111-111111111111")!
        let episodeID = UUID(uuidString: "22222222-2222-2222-2222-222222222222")!
        var note = Note(
            text: "Remember this moment",
            kind: .reflection,
            target: .episode(id: episodeID, positionSeconds: 42.5),
            author: .agent
        )
        note.id = noteID
        note.createdAt = Date(timeIntervalSince1970: 1_234)
        note.deleted = true

        let friendID = UUID(uuidString: "33333333-3333-3333-3333-333333333333")!
        var friend = Friend(displayName: "Alice", identifier: "aabbcc")
        friend.id = friendID
        friend.addedAt = Date(timeIntervalSince1970: 5_678)
        friend.avatarURL = "https://example.com/alice.png"
        friend.about = "Builds shows"

        let commands = SocialNativeStoreMigration.commands(
            from: .init(notes: [note], friends: [friend])
        )

        XCTAssertEqual(commands.count, 3)
        XCTAssertEqual(commands.map(\.namespace), [
            "podcast.social",
            "podcast.social",
            "podcast.social",
        ])

        let addNote = commands[0].body
        XCTAssertEqual(addNote["op"] as? String, "add_note")
        XCTAssertEqual(addNote["id"] as? String, noteID.uuidString)
        XCTAssertEqual(addNote["text"] as? String, "Remember this moment")
        XCTAssertEqual(addNote["kind"] as? String, "reflection")
        XCTAssertEqual(addNote["created_at"] as? Int, 1_234)
        XCTAssertEqual(addNote["author"] as? String, "agent")
        let target = try XCTUnwrap(addNote["target"] as? [String: Any])
        XCTAssertEqual(target["type"] as? String, "episode")
        XCTAssertEqual(target["episode_id"] as? String, episodeID.uuidString)
        XCTAssertEqual(target["position_secs"] as? Double, 42.5)

        let deleteNote = commands[1].body
        XCTAssertEqual(deleteNote["op"] as? String, "delete_note")
        XCTAssertEqual(deleteNote["id"] as? String, noteID.uuidString)

        let addFriend = commands[2].body
        XCTAssertEqual(addFriend["op"] as? String, "add_friend")
        XCTAssertEqual(addFriend["id"] as? String, friendID.uuidString)
        XCTAssertEqual(addFriend["display_name"] as? String, "Alice")
        XCTAssertEqual(addFriend["pubkey_hex"] as? String, "aabbcc")
        XCTAssertEqual(addFriend["added_at"] as? Int, 5_678)
        XCTAssertEqual(addFriend["avatar_url"] as? String, "https://example.com/alice.png")
        XCTAssertEqual(addFriend["about"] as? String, "Builds shows")
    }

    func testEmptyPayloadStillProducesNoCommands() {
        let commands = SocialNativeStoreMigration.commands(from: .init(notes: [], friends: []))
        XCTAssertTrue(commands.isEmpty)
    }
}
