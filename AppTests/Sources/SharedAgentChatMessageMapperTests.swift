import Pod0Core
import XCTest
@testable import Podcastr

final class SharedAgentChatMessageMapperTests: XCTestCase {
    func testMapsOldestTurnFirstWithStableMessageIdentifiers() {
        let older = turn(
            id: AgentTurnId(high: 1, low: 10),
            stage: .completed,
            messages: [
                AgentMessageProjection(role: .user, content: "First question"),
                AgentMessageProjection(role: .assistant, content: "First answer"),
            ]
        )
        let newer = turn(
            id: AgentTurnId(high: 1, low: 20),
            stage: .completed,
            messages: [AgentMessageProjection(role: .user, content: "Second question")]
        )

        let messages = SharedAgentChatMessageMapper.messages(from: [newer, older])

        XCTAssertEqual(messages.map(\.text), ["First question", "First answer", "Second question"])
        XCTAssertEqual(messages[0].id, older.turnId.messageUUID(at: 0))
        XCTAssertEqual(messages[1].id, older.turnId.messageUUID(at: 1))
    }

    func testToolPayloadIsNotRenderedAndSafeFailureIsVisible() {
        let projection = turn(
            id: AgentTurnId(high: 3, low: 40),
            stage: .failed,
            messages: [AgentMessageProjection(
                role: .tool,
                content: #"{"private":"provider payload"}"#
            )],
            safeFailure: "The action could not be completed."
        )

        let messages = SharedAgentChatMessageMapper.messages(from: [projection])

        XCTAssertEqual(messages.map(\.text), [
            "Agent action completed",
            "The action could not be completed.",
        ])
        XCTAssertEqual(messages[0].role, .toolBatch(batchID: messages[0].id, count: 1))
        XCTAssertEqual(messages[1].role, .error)
    }

    private func turn(
        id: AgentTurnId,
        stage: AgentTurnStage,
        messages: [AgentMessageProjection],
        safeFailure: String? = nil
    ) -> AgentTurnProjection {
        AgentTurnProjection(
            conversationId: ConversationId(high: 1, low: 2),
            turnId: id,
            revision: StateRevision(value: 1),
            stage: stage,
            messages: messages,
            proposal: nil,
            executionFenceId: nil,
            commit: nil,
            safeFailure: safeFailure,
            updatedAt: UnixTimestampMilliseconds(value: 1_900_000_000_000)
        )
    }
}
