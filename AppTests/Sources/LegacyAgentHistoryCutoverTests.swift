import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

final class LegacyAgentHistoryCutoverTests: XCTestCase {
    func testMapperPreservesStableConversationTurnAndErrorIdentity() throws {
        let base = Date(timeIntervalSince1970: 1_800_000_000)
        let conversationID = UUID()
        let firstTurnID = UUID()
        let secondTurnID = UUID()
        let conversation = ChatConversation(
            id: conversationID,
            title: "Architecture",
            messages: [
                ChatMessage(
                    id: firstTurnID,
                    role: .user,
                    text: "What mattered?",
                    timestamp: base
                ),
                ChatMessage(
                    role: .assistant,
                    text: "Shared decisions.",
                    timestamp: base.addingTimeInterval(1)
                ),
                ChatMessage(
                    id: secondTurnID,
                    role: .user,
                    text: "Try again",
                    timestamp: base.addingTimeInterval(2)
                ),
                ChatMessage(
                    role: .error,
                    text: "Provider unavailable",
                    timestamp: base.addingTimeInterval(3)
                ),
            ],
            createdAt: base,
            updatedAt: base.addingTimeInterval(3)
        )

        let mapped = try LegacyAgentHistoryMapper.map(
            LegacyAgentHistoryBackup(conversations: [conversation])
        )

        XCTAssertEqual(mapped.count, 1)
        XCTAssertEqual(mapped[0].conversationId, ConversationId(uuid: conversationID))
        XCTAssertEqual(mapped[0].title, "Architecture")
        XCTAssertEqual(mapped[0].turns.count, 2)
        XCTAssertEqual(mapped[0].turns[0].turnId, AgentTurnId(uuid: firstTurnID))
        XCTAssertEqual(mapped[0].turns[1].turnId, AgentTurnId(uuid: secondTurnID))
        XCTAssertEqual(mapped[0].turns[1].messages.last?.role, .error)
        XCTAssertEqual(
            mapped[0].turns[1].messages.last?.content,
            "Provider unavailable"
        )
    }

    func testMapperRejectsConversationWithoutLeadingUserMessage() {
        let base = Date(timeIntervalSince1970: 1_800_000_000)
        let conversation = ChatConversation(
            messages: [
                ChatMessage(role: .assistant, text: "Orphan", timestamp: base),
            ],
            createdAt: base,
            updatedAt: base
        )
        XCTAssertThrowsError(try LegacyAgentHistoryMapper.map(
            LegacyAgentHistoryBackup(conversations: [conversation])
        ))
    }

    func testBackupIsDeterministicAndConflictingRewriteFails() throws {
        let root = temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let originalConversation = conversation()
        let backup = LegacyAgentHistoryBackup(conversations: [originalConversation])
        let evidence = try backup.evidence()
        try backup.publish(to: root, sourceGeneration: 42)
        let restored = try LegacyAgentHistoryBackup.load(
            from: root,
            sourceGeneration: 42,
            expectedDigest: evidence.digest,
            expectedByteCount: evidence.byteCount
        )
        XCTAssertEqual(restored, backup)
        XCTAssertThrowsError(try LegacyAgentHistoryBackup(
            conversations: [conversation(title: "Changed")]
        ).publish(to: root, sourceGeneration: 42))
    }

    func testExactLegacySourceRetiresAndMismatchFailsClosed() throws {
        let root = temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let file = root.appendingPathComponent("chat.json")
        let expected = [conversation()]
        try encoded(expected).write(to: file, options: .atomic)
        let source = try LegacyChatHistorySource(fileURL: file)

        XCTAssertThrowsError(try source.retire(
            matching: [conversation(title: "Changed")]
        ))
        XCTAssertFalse(source.isRetired)
        try source.retire(matching: expected)
        XCTAssertTrue(source.isRetired)
        XCTAssertTrue(source.conversations.isEmpty)
    }

    func testMalformedFutureSourceDoesNotBecomeEmptyHistory() throws {
        let root = temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let file = root.appendingPathComponent("chat.json")
        try Data(#"{"schemaVersion":999,"conversations":[]}"#.utf8)
            .write(to: file, options: .atomic)
        XCTAssertThrowsError(try LegacyChatHistorySource(fileURL: file))
    }
}

private extension LegacyAgentHistoryCutoverTests {
    func conversation(title: String = "Original") -> ChatConversation {
        let base = Date(timeIntervalSince1970: 1_800_000_000)
        return ChatConversation(
            id: UUID(uuidString: "11111111-1111-1111-1111-111111111111")!,
            title: title,
            messages: [
                ChatMessage(
                    id: UUID(uuidString: "22222222-2222-2222-2222-222222222222")!,
                    role: .user,
                    text: "Remember this",
                    timestamp: base
                ),
                ChatMessage(
                    role: .assistant,
                    text: "Remembered",
                    timestamp: base.addingTimeInterval(1)
                ),
            ],
            createdAt: base,
            updatedAt: base.addingTimeInterval(1)
        )
    }

    func encoded(_ conversations: [ChatConversation]) throws -> Data {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        return try encoder.encode(conversations)
    }

    func temporaryDirectory() -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try! FileManager.default.createDirectory(
            at: url,
            withIntermediateDirectories: true
        )
        return url
    }
}
