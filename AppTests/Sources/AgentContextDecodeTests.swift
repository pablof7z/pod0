import XCTest
@testable import Podcastr

/// Guards the kernel → Swift decode of the agent-prompt inventory context.
/// The inventory selection / capping / filtering policy lives in the Rust
/// kernel and rides the snapshot as `agent_context`. `AgentPrompt` reads it
/// off `podcastSnapshot.agentContext`; a snake_case key mismatch would
/// silently default every field to empty/zero — the prompt would lose its
/// inventory while still building and passing every render test. This proves
/// the `.convertFromSnakeCase` decode path (used by `KernelBridge`).
final class AgentContextDecodeTests: XCTestCase {

    /// Decode `PodcastUpdate` exactly the way `KernelBridge` does.
    private func decode(_ json: String) throws -> PodcastUpdate {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try decoder.decode(PodcastUpdate.self, from: Data(json.utf8))
    }

    func testAgentContextDecodesFromSnakeCaseKeys() throws {
        // Mirrors the Rust `AgentContextSnapshot` wire shape.
        let json = """
        {
          "rev": 7,
          "agent_context": {
            "subscriptions": ["Acquired", "Lex Fridman"],
            "subscriptions_total": 5,
            "in_progress": [
              {"title": "Resumed Ep", "show_title": "Lex Fridman"}
            ],
            "recent_unplayed": [
              {"title": "Fresh Ep", "show_title": "Acquired"}
            ],
            "recent_window_days": 7
          }
        }
        """
        let update = try decode(json)
        let ctx = try XCTUnwrap(
            update.agentContext,
            "agent_context must decode through .convertFromSnakeCase — nil means the key did not match"
        )
        XCTAssertEqual(ctx.subscriptions, ["Acquired", "Lex Fridman"])
        XCTAssertEqual(ctx.subscriptionsTotal, 5)
        XCTAssertEqual(ctx.inProgress.first?.title, "Resumed Ep")
        XCTAssertEqual(ctx.inProgress.first?.showTitle, "Lex Fridman")
        XCTAssertEqual(ctx.recentUnplayed.first?.title, "Fresh Ep")
        XCTAssertEqual(ctx.recentUnplayed.first?.showTitle, "Acquired")
        XCTAssertEqual(ctx.recentWindowDays, 7)
    }

    func testAgentContextOmittedDecodesToNil() throws {
        // Fresh install / empty library: the kernel omits `agent_context`.
        let update = try decode("{\"rev\": 1}")
        XCTAssertNil(update.agentContext)
    }

    func testAgentContextEmptyCollectionsDecodeToDefaults() throws {
        // Rust skips empty Vecs on the wire; the always-present scalars remain.
        let json = """
        {"rev": 2, "agent_context": {"subscriptions_total": 0, "recent_window_days": 7}}
        """
        let ctx = try XCTUnwrap(try decode(json).agentContext)
        XCTAssertTrue(ctx.subscriptions.isEmpty)
        XCTAssertTrue(ctx.inProgress.isEmpty)
        XCTAssertTrue(ctx.recentUnplayed.isEmpty)
        XCTAssertEqual(ctx.recentWindowDays, 7)
    }

    func testAgentTaskSnapshotDecodesWithoutRawDispatchFields() throws {
        let json = """
        {
          "rev": 3,
          "agent_tasks": [
            {
              "id": "task-1",
              "title": "Inbox Triage",
              "intent_type": "inbox_triage",
              "intent_label": "Triage inbox",
              "intent_detail": "Prioritize new episodes",
              "schedule": "daily",
              "status": "pending",
              "is_enabled": true
            }
          ]
        }
        """
        let task = try XCTUnwrap(try decode(json).agentTasks.first)
        XCTAssertEqual(task.title, "Inbox Triage")
        XCTAssertEqual(task.intentType, "inbox_triage")
        XCTAssertEqual(task.intentLabel, "Triage inbox")
    }

    func testLocalNotesDecodeFromSnakeCaseKeys() throws {
        let json = """
        {
          "rev": 4,
          "notes": [
            {
              "id": "note-1",
              "text": "Remember this",
              "kind": "free",
              "target": {
                "type": "episode",
                "episode_id": "ep-1",
                "position_secs": 12.5
              },
              "created_at": 123,
              "deleted": false,
              "author": "user"
            }
          ]
        }
        """
        let note = try XCTUnwrap(try decode(json).notes.first)
        XCTAssertEqual(note.id, "note-1")
        XCTAssertEqual(note.text, "Remember this")
        XCTAssertEqual(note.target?.type, "episode")
        XCTAssertEqual(note.target?.episodeId, "ep-1")
        XCTAssertEqual(note.target?.positionSecs, 12.5)
    }
}
