import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class SharedAgentConversationSessionTests: XCTestCase {
    func testStartSubmitsTypedBoundedCommandAndRendersProjection() async {
        let runtime = StubSharedAgentConversationRuntime()
        let session = SharedAgentConversationSession(
            runtime: runtime,
            modelReference: { "openrouter/test" }
        )

        await session.startTurn("  What should I hear next?  ")

        XCTAssertEqual(runtime.commands, [
            .startAgentTurn(
                conversationId: nil,
                userInput: "What should I hear next?",
                modelReference: "openrouter/test",
                availableTools: SharedAgentConversationSession.productProofTools
            ),
        ])
        XCTAssertEqual(session.phase, .running)
        runtime.emit(conversation(stage: .completed, revision: 2))
        await Task.yield()
        XCTAssertEqual(session.phase, .idle)
        XCTAssertEqual(
            session.messages.map(\.content),
            ["What should I hear next?", "Try the architecture episode."]
        )
    }

    func testCancellationUsesExactActiveTurnRevision() async {
        let runtime = StubSharedAgentConversationRuntime()
        let session = SharedAgentConversationSession(
            runtime: runtime,
            modelReference: { "openrouter/test" }
        )
        await session.startTurn("Pause playback")
        runtime.emit(conversation(stage: .awaitingModel, revision: 7))
        await Task.yield()

        await session.cancelActiveTurn()

        XCTAssertEqual(runtime.commands.last, .cancelAgentTurn(
            turnId: AgentTurnId(high: 3, low: 4),
            expectedTurnRevision: StateRevision(value: 7)
        ))
    }

    func testStaleProjectionCannotReplaceNewerConversationState() async {
        let runtime = StubSharedAgentConversationRuntime()
        let session = SharedAgentConversationSession(
            runtime: runtime,
            modelReference: { "openrouter/test" }
        )
        await session.startTurn("Remember this")
        runtime.emit(conversation(stage: .completed, revision: 9), stateRevision: 9)
        runtime.emit(conversation(stage: .awaitingModel, revision: 1), stateRevision: 8)
        await Task.yield()

        XCTAssertEqual(session.phase, .idle)
        XCTAssertEqual(session.stateRevision, 9)
    }

    func testResumesPersistedConversationAndClearsPointerForNewConversation() {
        let runtime = StubSharedAgentConversationRuntime()
        let resumedID = ConversationId(high: 8, low: 9)
        var changes: [ConversationId?] = []
        let session = SharedAgentConversationSession(
            runtime: runtime,
            resumeConversationID: resumedID,
            onConversationChanged: { changes.append($0) },
            modelReference: { "openrouter/test" }
        )

        XCTAssertEqual(runtime.subscribedConversationID, resumedID)
        XCTAssertEqual(changes, [resumedID])

        session.startNewConversation()

        XCTAssertNil(runtime.subscribedConversationID)
        XCTAssertEqual(changes, [resumedID, nil])
    }

    private func conversation(
        stage: AgentTurnStage,
        revision: UInt64
    ) -> AgentConversationProjection {
        AgentConversationProjection(
            conversationId: ConversationId(high: 1, low: 2),
            turns: [AgentTurnProjection(
                conversationId: ConversationId(high: 1, low: 2),
                turnId: AgentTurnId(high: 3, low: 4),
                revision: StateRevision(value: revision),
                stage: stage,
                messages: [
                    AgentMessageProjection(role: .user, content: "What should I hear next?"),
                    AgentMessageProjection(
                        role: .assistant,
                        content: "Try the architecture episode."
                    ),
                ],
                proposal: nil,
                executionFenceId: nil,
                commit: nil,
                safeFailure: nil,
                updatedAt: UnixTimestampMilliseconds(value: 1_900_000_000_000)
            )],
            hasMore: false,
            failure: nil
        )
    }
}

@MainActor
private final class StubSharedAgentConversationRuntime: SharedAgentConversationRuntime {
    private var subscriber: (any ProjectionSubscriber)?
    private(set) var commands: [ApplicationCommand] = []
    private(set) var subscribedConversationID: ConversationId?

    func execute(_ command: ApplicationCommand) async throws -> OperationResult? {
        commands.append(command)
        switch command {
        case .startAgentTurn:
            return .agentTurnStarted(
                conversationId: ConversationId(high: 1, low: 2),
                turnId: AgentTurnId(high: 3, low: 4)
            )
        case .cancelAgentTurn:
            return nil
        default:
            throw StubError.unexpectedCommand
        }
    }

    func subscribeAgentConversation(
        _ conversationID: ConversationId,
        subscriber: any ProjectionSubscriber
    ) -> SubscriptionId {
        self.subscriber = subscriber
        subscribedConversationID = conversationID
        return SubscriptionId(high: conversationID.high, low: conversationID.low)
    }

    func unsubscribeAgentConversation(_ subscriptionID: SubscriptionId) {
        subscriber = nil
        subscribedConversationID = nil
    }

    func executePendingHostRequests() {}

    func emit(_ conversation: AgentConversationProjection, stateRevision: UInt64 = 1) {
        subscriber?.receive(projection: ProjectionEnvelope(
            contractVersion: 1,
            stateRevision: StateRevision(value: stateRevision),
            projection: .agentConversation(value: conversation)
        ))
    }

    private enum StubError: Error {
        case unexpectedCommand
    }
}
