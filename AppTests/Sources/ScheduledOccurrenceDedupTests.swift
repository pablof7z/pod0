import Foundation
import XCTest
@testable import Podcastr

@MainActor
final class ScheduledOccurrenceDedupTests: XCTestCase {
    func testLegacySourcePreservesResumableOccurrenceConversation() throws {
        let fileURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("\(UUID().uuidString).json")
        defer { try? FileManager.default.removeItem(at: fileURL) }
        let occurrence = "scheduled:task:1000"
        let first = ChatConversation(
            id: OccurrenceIdentity.uuid(for: occurrence),
            title: "First",
            messages: [.init(role: .user, text: "Run")],
            isScheduledTask: true,
            occurrenceID: occurrence
        )
        var resumed = first
        resumed.messages.append(.init(role: .assistant, text: "Finished"))
        resumed.updatedAt = Date().addingTimeInterval(1)
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        try encoder.encode([resumed]).write(to: fileURL, options: .atomic)
        let history = try LegacyChatHistorySource(fileURL: fileURL)
        let restored = history.conversations.first {
            $0.occurrenceID == occurrence
        }

        XCTAssertEqual(history.conversations.count, 1)
        XCTAssertEqual(restored?.messages.count, 2)
        XCTAssertTrue(restored?.hasCompletedScheduledOutput == true)
        XCTAssertEqual(restored?.id, OccurrenceIdentity.uuid(for: occurrence))
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
