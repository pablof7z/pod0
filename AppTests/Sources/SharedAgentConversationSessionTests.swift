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
                modelReference: "openrouter/test"
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

    func testCommittedProjectionEmitsIdempotentAgentCompletionSignal() async {
        let runtime = StubSharedAgentConversationRuntime()
        let signals = RecordingProductSignalSink()
        let session = SharedAgentConversationSession(
            runtime: runtime,
            productSignals: signals,
            modelReference: { "openrouter/test" }
        )
        await session.startTurn("What should I hear next?")
        let projection = conversation(stage: .completed, revision: 2)

        runtime.emit(projection)
        runtime.emit(projection)
        let captured = await signals.waitForCount(1)

        XCTAssertEqual(captured.map(\.name), [.agentTurnCompleted])
        await Task.yield()
        let replayed = await signals.captured()
        XCTAssertEqual(replayed.count, 1)
    }

    func testHistoricalTerminalTurnDoesNotBackfillAfterSubscription() async throws {
        let runtime = StubSharedAgentConversationRuntime()
        let signals = RecordingProductSignalSink()
        let session = SharedAgentConversationSession(
            runtime: runtime,
            productSignals: signals,
            resumeConversationID: ConversationId(high: 1, low: 2),
            modelReference: { "openrouter/test" }
        )
        try await waitUntil { runtime.subscribedConversationID != nil }

        runtime.emit(conversation(stage: .completed, revision: 2))
        await Task.yield()

        let captured = await signals.captured()
        XCTAssertTrue(captured.isEmpty)
        _ = session
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

    func testResumesPersistedConversationAndClearsPointerForNewConversation() async throws {
        let runtime = StubSharedAgentConversationRuntime()
        let resumedID = ConversationId(high: 8, low: 9)
        var changes: [ConversationId?] = []
        let session = SharedAgentConversationSession(
            runtime: runtime,
            resumeConversationID: resumedID,
            onConversationChanged: { changes.append($0) },
            modelReference: { "openrouter/test" }
        )

        try await waitUntil { runtime.subscribedConversationID == resumedID }
        XCTAssertEqual(changes, [resumedID])

        session.startNewConversation()

        try await waitUntil { runtime.subscribedConversationID == nil }
        XCTAssertEqual(changes, [resumedID, nil])
    }

    func testLoadsAndRefreshesDurableConversationHistory() async throws {
        let runtime = StubSharedAgentConversationRuntime()
        runtime.history = historyProjection(
            id: ConversationId(high: 11, low: 12),
            title: "What did they say about habits?"
        )
        let session = SharedAgentConversationSession(
            runtime: runtime,
            modelReference: { "openrouter/test" }
        )

        try await waitUntil {
            session.conversationSummaries.map(\.title) ==
                ["What did they say about habits?"]
        }

        runtime.history = historyProjection(
            id: ConversationId(high: 21, low: 22),
            title: "What should I hear next?"
        )
        session.refreshConversationHistory()

        try await waitUntil {
            session.conversationSummaries.map(\.conversationId) ==
                [ConversationId(high: 21, low: 22)]
        }
    }

    private func waitUntil(
        _ condition: @escaping @MainActor () -> Bool
    ) async throws {
        for _ in 0 ..< 100 {
            if condition() { return }
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTFail("Condition did not become true")
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
                recallEvidence: [],
                modelUsage: [],
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

    private func historyProjection(
        id: ConversationId,
        title: String
    ) -> AgentConversationsProjection {
        AgentConversationsProjection(
            conversations: [AgentConversationSummaryProjection(
                conversationId: id,
                title: title,
                preview: "A durable answer",
                turnCount: 1,
                latestStage: .completed,
                createdAt: UnixTimestampMilliseconds(value: 1_900_000_000_000),
                updatedAt: UnixTimestampMilliseconds(value: 1_900_000_001_000)
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
    var history = AgentConversationsProjection(
        conversations: [],
        hasMore: false,
        failure: nil
    )

    func agentConversationHistory() async -> AgentConversationsProjection {
        history
    }

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
    ) async -> SubscriptionId {
        self.subscriber = subscriber
        subscribedConversationID = conversationID
        return SubscriptionId(high: conversationID.high, low: conversationID.low)
    }

    func unsubscribeAgentConversation(_ subscriptionID: SubscriptionId) async {
        subscriber = nil
        subscribedConversationID = nil
    }

    func executePendingHostRequests() {}

    func emit(_ conversation: AgentConversationProjection, stateRevision: UInt64 = 1) {
        subscriber?.receive(projection: ProjectionEnvelope(
            contractVersion: 1,
            stateRevision: StateRevision(value: stateRevision),
            contentChanged: true,
            projection: .agentConversation(value: conversation)
        ))
    }

    private enum StubError: Error {
        case unexpectedCommand
    }
}
