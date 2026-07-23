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
            )]
        ))
        let host = makeHost(transport: transport)

        let observation = await host.execute(.executeAgentModelTurn(execution: modelRequest()))

        guard case .agentModelCompleted(
            let turnID,
            let fenceID,
            let text,
            let call
        ) = observation else {
            return XCTFail("Expected raw model completion")
        }
        XCTAssertEqual(turnID, AgentTurnId(high: 3, low: 4))
        XCTAssertEqual(fenceID, AgentExecutionFenceId(high: 5, low: 6))
        XCTAssertEqual(text, "I'll save that.")
        XCTAssertEqual(call?.providerCallId, "call-1")
        XCTAssertEqual(call?.toolName, "create_note")
        XCTAssertEqual(call?.argumentsJson, #"{"text":"Architecture matters"}"#)
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

    private func makeHost(
        transport: StubCoreAgentModelTransport,
        approval: (any CoreAgentApprovalPresenting)? = nil
    ) -> CoreAgentHost {
        CoreAgentHost(
            modelTransport: transport,
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
private final class StubCoreAgentModelTransport: CoreAgentModelTransporting {
    let result: AgentResult
    private(set) var lastMessages: [[String: Any]] = []
    private(set) var lastTools: [[String: Any]] = []

    init(result: AgentResult) {
        self.result = result
    }

    func complete(
        messages: [[String: Any]],
        tools: [[String: Any]],
        model _: String,
        ollamaChatURL _: URL?
    ) async throws -> AgentResult {
        lastMessages = messages
        lastTools = tools
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
