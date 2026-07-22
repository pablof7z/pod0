import Foundation
import XCTest
@testable import Podcastr

@MainActor
final class ScheduledOccurrenceDedupTests: XCTestCase {
    func testVisibleConversationIsUniqueAndResumableByOccurrence() {
        let fileURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("\(UUID().uuidString).json")
        defer { try? FileManager.default.removeItem(at: fileURL) }
        let history = ChatHistoryStore(fileURL: fileURL)
        let occurrence = "scheduled:task:1000"
        let first = ChatConversation(
            id: OccurrenceIdentity.uuid(for: occurrence),
            title: "First",
            messages: [.init(role: .user, text: "Run")],
            isScheduledTask: true,
            occurrenceID: occurrence
        )
        history.upsert(first)
        XCTAssertFalse(history.conversation(
            occurrenceID: occurrence
        )?.hasCompletedScheduledOutput == true)
        var resumed = first
        resumed.messages.append(.init(role: .assistant, text: "Finished"))
        resumed.updatedAt = Date().addingTimeInterval(1)
        history.upsert(resumed)

        XCTAssertEqual(history.conversations.count, 1)
        XCTAssertEqual(history.conversation(occurrenceID: occurrence)?.messages.count, 2)
        XCTAssertTrue(history.conversation(
            occurrenceID: occurrence
        )?.hasCompletedScheduledOutput == true)
        XCTAssertEqual(
            history.conversation(occurrenceID: occurrence)?.id,
            OccurrenceIdentity.uuid(for: occurrence)
        )
    }

    func testScheduledOutputRejectsPartialAssistantFollowedByError() {
        let occurrence = "scheduled:task:interrupted"
        let conversation = ChatConversation(
            id: OccurrenceIdentity.uuid(for: occurrence),
            messages: [
                .init(role: .user, text: "Run"),
                .init(role: .assistant, text: "Partial"),
                .init(role: .error, text: "Connection lost"),
            ],
            isScheduledTask: true,
            occurrenceID: occurrence
        )

        XCTAssertFalse(conversation.hasCompletedScheduledOutput)
    }

    func testQualifiedOutputAccessorReturnsTheSameCompletionEvidence() {
        let occurrence = "scheduled:task:complete"
        let conversation = ChatConversation(
            messages: [
                .init(role: .user, text: "Run"),
                .init(role: .assistant, text: "Finished output"),
            ],
            isScheduledTask: true,
            occurrenceID: occurrence
        )
        XCTAssertEqual(conversation.completedScheduledOutputText, "Finished output")
        XCTAssertTrue(conversation.hasCompletedScheduledOutput)
    }
}
