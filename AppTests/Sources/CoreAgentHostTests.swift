import Pod0Core
import XCTest
@testable import Podcastr
@MainActor
final class CoreAgentHostTests: XCTestCase {
    func testModelCompletionReturnsBoundedRawToolCallWithoutNativeParsing() async throws {
        let transport = StubCoreAgentModelTransport(result: AgentResult(
            assistantMessage: ["role": "assistant", "content": "I'll save that."],
            toolCalls: [AgentToolCall(
                id: "call-1",
                name: "create_note",
                arguments: #"{"text":"Architecture matters"}"#
            )],
            tokensUsed: AgentTokenUsage(promptTokens: 120, completionTokens: 24, cachedTokens: 40)
        ))
        let host = makeHost(transport: transport)
        let observation = await host.execute(.executeAgentModelTurn(execution: modelRequest()))

        guard case .agentModelCompleted(
            let turnID,
            let fenceID,
            let text,
            let call,
            let usage
        ) = observation else {
            return XCTFail("Expected raw model completion")
        }
        XCTAssertEqual(turnID, AgentTurnId(high: 3, low: 4))
        XCTAssertEqual(fenceID, AgentExecutionFenceId(high: 5, low: 6))
        XCTAssertEqual(text, "I'll save that.")
        XCTAssertEqual(call?.providerCallId, "call-1")
        XCTAssertEqual(call?.toolName, "create_note")
        XCTAssertEqual(call?.argumentsJson, #"{"text":"Architecture matters"}"#)
        XCTAssertEqual(usage?.promptTokens, 120)
        XCTAssertEqual(usage?.completionTokens, 24)
        XCTAssertEqual(usage?.cachedPromptTokens, 40)
        XCTAssertEqual(transport.lastTools.count, 1)
        XCTAssertEqual(transport.lastMessages.first?["role"] as? String, "system")
    }
    func testMultipleToolCallsFailBeforeCrossingFacade() async {
        let transport = StubCoreAgentModelTransport(result: AgentResult(
            assistantMessage: ["role": "assistant"],
            toolCalls: [
                AgentToolCall(id: "1", name: "create_note", arguments: "{}"),
                AgentToolCall(id: "2", name: "create_note", arguments: "{}"),
            ]
        ))
        let observation = await makeHost(transport: transport).execute(
            .executeAgentModelTurn(execution: modelRequest())
        )
        guard case .failed(let code, _) = observation else {
            return XCTFail("Expected model failure")
        }
        XCTAssertEqual(code, .invalidResponse)
    }
    func testProviderAdapterPreservesPriorToolEvidenceWithoutForgingCallID() async {
        let transport = StubCoreAgentModelTransport(result: AgentResult(
            assistantMessage: ["role": "assistant", "content": "That note was saved."],
            toolCalls: []
        ))
        let request = AgentModelExecutionRequest(
            conversationId: ConversationId(high: 1, low: 2),
            turnId: AgentTurnId(high: 3, low: 4),
            modelFenceId: AgentExecutionFenceId(high: 5, low: 6),
            modelReference: "openrouter/test",
            messages: [
                AgentMessageProjection(role: .user, content: "Save this"),
                AgentMessageProjection(role: .assistant, content: "I'll save it."),
                AgentMessageProjection(role: .tool, content: #"{"saved":true}"#),
                AgentMessageProjection(role: .user, content: "Did that work?"),
            ],
            availableTools: [.createNote],
            maximumOutputBytes: 1_024
        )

        _ = await makeHost(transport: transport).execute(.executeAgentModelTurn(execution: request))

        XCTAssertEqual(
            transport.lastMessages.compactMap { $0["role"] as? String },
            ["system", "user", "assistant", "system", "user"]
        )
        XCTAssertEqual(
            transport.lastMessages[3]["content"] as? String,
            "Tool result:\n{\"saved\":true}"
        )
        XCTAssertNil(transport.lastMessages[3]["tool_call_id"])
    }
    func testFinalAnswerContinuationCrossesNativeHostWithoutToolSchemas() async {
        let transport = StubCoreAgentModelTransport(result: AgentResult(
            assistantMessage: ["role": "assistant", "content": "Saved that note."],
            toolCalls: []
        ))
        let request = AgentModelExecutionRequest(
            conversationId: ConversationId(high: 1, low: 2),
            turnId: AgentTurnId(high: 3, low: 4),
            modelFenceId: AgentExecutionFenceId(high: 5, low: 6),
            modelReference: "openrouter/test",
            messages: [
                AgentMessageProjection(role: .user, content: "Save this"),
                AgentMessageProjection(role: .tool, content: #"{"saved":true}"#),
            ],
            availableTools: [],
            maximumOutputBytes: 1_024
        )

        let observation = await makeHost(transport: transport).execute(
            .executeAgentModelTurn(execution: request)
        )

        guard case .agentModelCompleted(_, _, let text, let call, _) = observation else {
            return XCTFail("Expected final model completion")
        }
        XCTAssertEqual(text, "Saved that note.")
        XCTAssertNil(call)
        XCTAssertTrue(transport.lastTools.isEmpty)
    }
    func testStreamingPresentationIsBoundedAndRetainedUntilProjectionArrives() async {
        let streaming = CoreAgentStreamingState()
        let transport = StubCoreAgentModelTransport(
            result: AgentResult(
                assistantMessage: ["role": "assistant", "content": "Complete response"],
                toolCalls: []
            ),
            partialContent: "Partial response"
        )
        let host = CoreAgentHost(
            modelTransport: transport,
            streamingState: streaming,
            approvalPresenter: nil,
            systemPrompt: { "You are Pod0." },
            ollamaURL: { nil }
        )

        _ = await host.execute(.executeAgentModelTurn(execution: modelRequest()))

        XCTAssertEqual(streaming.turnID, AgentTurnId(high: 3, low: 4))
        XCTAssertEqual(streaming.content, "Partial response")
        XCTAssertFalse(streaming.isActive)
        streaming.clear(turnID: AgentTurnId(high: 3, low: 4))
        XCTAssertNil(streaming.content)
    }
    func testApprovalObservationEchoesExactRustProposalIdentity() async {
        let presenter = StubCoreAgentApprovalPresenter(approved: true)
        let host = makeHost(
            transport: StubCoreAgentModelTransport(result: AgentResult(
                assistantMessage: [:],
                toolCalls: []
            )),
            approval: presenter
        )
        let request = approvalRequest()

        let observation = await host.execute(.presentAgentApproval(approval: request))

        guard case .agentApprovalObserved(
            let turnID,
            let proposalID,
            let digest,
            let approved
        ) = observation else {
            return XCTFail("Expected approval observation")
        }
        XCTAssertTrue(approved)
        XCTAssertEqual(turnID, request.turnId)
        XCTAssertEqual(proposalID, request.proposal.proposalId)
        XCTAssertEqual(digest, request.proposal.proposalDigest)
        XCTAssertEqual(presenter.lastRequest, request)
    }
    func testCapabilityObservationEchoesExactRustExecutionFence() async {
        let executor = StubCoreAgentCapabilityExecutor(
            outcome: .succeeded(boundedResult: #"{"paused":true}"#)
        )
        let host = makeHost(
            transport: StubCoreAgentModelTransport(result: AgentResult(
                assistantMessage: [:],
                toolCalls: []
            )),
            capability: executor
        )
        let request = AgentCapabilityRequest(
            turnId: AgentTurnId(high: 1, low: 2),
            proposalId: AgentProposalId(high: 3, low: 4),
            proposalDigest: ContentDigest(word0: 5, word1: 6, word2: 7, word3: 8),
            executionFenceId: AgentExecutionFenceId(high: 9, low: 10),
            action: .noArguments(tool: .pausePlayback)
        )

        let observation = await host.execute(.executeAgentCapability(capability: request))

        guard case .agentCapabilityObserved(
            let turnID,
            let proposalID,
            let fenceID,
            let outcome
        ) = observation else {
            return XCTFail("Expected capability observation")
        }
        XCTAssertEqual(turnID, request.turnId)
        XCTAssertEqual(proposalID, request.proposalId)
        XCTAssertEqual(fenceID, request.executionFenceId)
        XCTAssertEqual(outcome, .succeeded(boundedResult: #"{"paused":true}"#))
        XCTAssertEqual(executor.lastAction, request.action)
    }

    private func makeHost(
        transport: StubCoreAgentModelTransport,
        approval: (any CoreAgentApprovalPresenting)? = nil,
        capability: any CoreAgentCapabilityExecuting = UnavailableCoreAgentCapabilityExecutor()
    ) -> CoreAgentHost {
        CoreAgentHost(
            modelTransport: transport,
            capabilityExecutor: capability,
            approvalPresenter: approval,
            systemPrompt: { "You are Pod0." },
            ollamaURL: { nil }
        )
    }

    private func modelRequest() -> AgentModelExecutionRequest {
        AgentModelExecutionRequest(
            conversationId: ConversationId(high: 1, low: 2),
            turnId: AgentTurnId(high: 3, low: 4),
            modelFenceId: AgentExecutionFenceId(high: 5, low: 6),
            modelReference: "openrouter/test",
            messages: [AgentMessageProjection(role: .user, content: "Save a note")],
            availableTools: [.createNote],
            maximumOutputBytes: 1_024
        )
    }
}

@MainActor
private final class StubCoreAgentCapabilityExecutor: CoreAgentCapabilityExecuting {
    let outcome: AgentCapabilityOutcome
    private(set) var lastAction: AgentToolAction?

    init(outcome: AgentCapabilityOutcome) {
        self.outcome = outcome
    }

    func execute(_ action: AgentToolAction) async -> AgentCapabilityOutcome {
        lastAction = action
        return outcome
    }
}

@MainActor
private final class StubCoreAgentModelTransport: CoreAgentModelTransporting {
    let result: AgentResult
    let partialContent: String?
    private(set) var lastMessages: [[String: Any]] = []
    private(set) var lastTools: [[String: Any]] = []

    init(result: AgentResult, partialContent: String? = nil) {
        self.result = result
        self.partialContent = partialContent
    }

    func complete(
        messages: [[String: Any]],
        tools: [[String: Any]],
        model _: String,
        ollamaChatURL _: URL?,
        onPartialContent: @escaping (String) -> Void
    ) async throws -> AgentResult {
        lastMessages = messages
        lastTools = tools
        if let partialContent { onPartialContent(partialContent) }
        return result
    }
}

@MainActor
private final class StubCoreAgentApprovalPresenter: CoreAgentApprovalPresenting {
    let approved: Bool
    private(set) var lastRequest: AgentApprovalRequest?

    init(approved: Bool) {
        self.approved = approved
    }

    func requestApproval(_ request: AgentApprovalRequest) async -> Bool {
        lastRequest = request
        return approved
    }
}

func approvalRequest() -> AgentApprovalRequest {
    AgentApprovalRequest(
        turnId: AgentTurnId(high: 3, low: 4),
        proposal: AgentProposalProjection(
            proposalId: AgentProposalId(high: 7, low: 8),
            proposalDigest: ContentDigest(word0: 1, word1: 2, word2: 3, word3: 4),
            revision: StateRevision(value: 2),
            action: .createNote(text: "Architecture matters"),
            requiredAuthority: .durableTurnGrant
        )
    )
}
