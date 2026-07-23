import Foundation
import Pod0Core

@MainActor
protocol CoreAgentHosting: AnyObject {
    func execute(_ request: HostRequest) async -> HostObservation
}

@MainActor
protocol CoreAgentModelTransporting: AnyObject {
    func complete(
        messages: [[String: Any]],
        tools: [[String: Any]],
        model: String,
        ollamaChatURL: URL?,
        onPartialContent: @escaping (String) -> Void
    ) async throws -> AgentResult
}

@MainActor
protocol CoreAgentApprovalPresenting: AnyObject {
    func requestApproval(_ request: AgentApprovalRequest) async -> Bool
}

@MainActor
final class CoreAgentHost: CoreAgentHosting {
    typealias Prompt = @MainActor () -> String
    typealias OllamaURL = @MainActor () -> URL?

    private let modelTransport: any CoreAgentModelTransporting
    private let capabilityExecutor: any CoreAgentCapabilityExecuting
    private let streamingState: CoreAgentStreamingState
    private weak var approvalPresenter: (any CoreAgentApprovalPresenting)?
    private let systemPrompt: Prompt
    private let ollamaURL: OllamaURL

    init(
        modelTransport: any CoreAgentModelTransporting = LiveCoreAgentModelTransport(),
        capabilityExecutor: any CoreAgentCapabilityExecuting = UnavailableCoreAgentCapabilityExecutor(),
        streamingState: CoreAgentStreamingState = CoreAgentStreamingState(),
        approvalPresenter: (any CoreAgentApprovalPresenting)?,
        systemPrompt: @escaping Prompt,
        ollamaURL: @escaping OllamaURL
    ) {
        self.modelTransport = modelTransport
        self.capabilityExecutor = capabilityExecutor
        self.streamingState = streamingState
        self.approvalPresenter = approvalPresenter
        self.systemPrompt = systemPrompt
        self.ollamaURL = ollamaURL
    }

    func execute(_ request: HostRequest) async -> HostObservation {
        switch request {
        case .executeAgentModelTurn(let execution):
            return await executeModel(execution)
        case .presentAgentApproval(let approval):
            return await presentApproval(approval)
        case .executeAgentCapability(let capability):
            let outcome = await capabilityExecutor.execute(capability.action)
            return .agentCapabilityObserved(
                turnId: capability.turnId,
                proposalId: capability.proposalId,
                executionFenceId: capability.executionFenceId,
                outcome: outcome
            )
        default:
            return .failed(
                code: .invalidResponse,
                safeDetail: "Non-agent request sent to agent host"
            )
        }
    }

    private func executeModel(_ execution: AgentModelExecutionRequest) async -> HostObservation {
        guard let messages = Self.providerMessages(
            systemPrompt: systemPrompt(),
            messages: execution.messages
        ), let tools = CoreAgentToolSchemas.schemas(for: execution.availableTools)
        else {
            return .failed(
                code: .invalidResponse,
                safeDetail: "Agent model request uses an unsupported contract"
            )
        }
        streamingState.begin(
            turnID: execution.turnId,
            fenceID: execution.modelFenceId,
            maximumBytes: execution.maximumOutputBytes
        )
        do {
            let result = try await modelTransport.complete(
                messages: messages,
                tools: tools,
                model: execution.modelReference,
                ollamaChatURL: ollamaURL(),
                onPartialContent: { [streamingState] content in
                    streamingState.update(
                        turnID: execution.turnId,
                        fenceID: execution.modelFenceId,
                        content: content
                    )
                }
            )
            streamingState.finish(
                turnID: execution.turnId,
                fenceID: execution.modelFenceId
            )
            guard result.toolCalls.count <= 1 else {
                return .failed(
                    code: .invalidResponse,
                    safeDetail: "Agent model returned multiple tool calls"
                )
            }
            let assistantText = result.assistantMessage["content"] as? String ?? ""
            guard UInt64(assistantText.utf8.count) <= execution.maximumOutputBytes else {
                return .failed(
                    code: .responseTooLarge,
                    safeDetail: "Agent model response exceeds the core limit"
                )
            }
            return .agentModelCompleted(
                turnId: execution.turnId,
                modelFenceId: execution.modelFenceId,
                assistantText: assistantText,
                proposedToolCall: result.toolCalls.first.map {
                    AgentModelToolCallObservation(
                        providerCallId: $0.id,
                        toolName: $0.name,
                        argumentsJson: $0.arguments
                    )
                },
                usage: Self.usage(from: result.tokensUsed)
            )
        } catch is CancellationError {
            streamingState.clear(turnID: execution.turnId)
            return .cancelled
        } catch let error as AgentError {
            streamingState.clear(turnID: execution.turnId)
            return Self.failure(for: error)
        } catch {
            streamingState.clear(turnID: execution.turnId)
            return .failed(
                code: .providerUnavailable,
                safeDetail: "Agent model provider request failed"
            )
        }
    }

    private static func usage(from usage: AgentTokenUsage?) -> AgentModelUsageObservation? {
        guard let usage,
              (0...1_000_000_000).contains(usage.promptTokens),
              (0...1_000_000_000).contains(usage.completionTokens)
        else {
            return nil
        }
        if let cached = usage.cachedTokens,
           !(0...usage.promptTokens).contains(cached) {
            return nil
        }
        return AgentModelUsageObservation(
            promptTokens: UInt64(usage.promptTokens),
            completionTokens: UInt64(usage.completionTokens),
            cachedPromptTokens: usage.cachedTokens.map(UInt64.init)
        )
    }

    private func presentApproval(_ approval: AgentApprovalRequest) async -> HostObservation {
        guard let approvalPresenter else {
            return .failed(
                code: .platformFailure,
                safeDetail: "Agent approval presentation is unavailable"
            )
        }
        let approved = await approvalPresenter.requestApproval(approval)
        return .agentApprovalObserved(
            turnId: approval.turnId,
            proposalId: approval.proposal.proposalId,
            proposalDigest: approval.proposal.proposalDigest,
            approved: approved
        )
    }

    private static func providerMessages(
        systemPrompt: String,
        messages: [AgentMessageProjection]
    ) -> [[String: Any]]? {
        guard !systemPrompt.isBlank else { return nil }
        var result: [[String: Any]] = [["role": "system", "content": systemPrompt]]
        for message in messages {
            let role: String
            let content: String
            switch message.role {
            case .user:
                role = "user"
                content = message.content
            case .assistant:
                role = "assistant"
                content = message.content
            case .tool:
                // The shared contract intentionally omits provider-specific
                // tool-call ids. Preserve the evidence as bounded system
                // context instead of inventing an OpenAI tool-call pairing.
                role = "system"
                content = "Tool result:\n\(message.content)"
            }
            result.append(["role": role, "content": content])
        }
        return result
    }

    private static func failure(for error: AgentError) -> HostObservation {
        switch error {
        case .missingCredential:
            .failed(code: .unauthorized, safeDetail: "Agent model credential unavailable")
        case .invalidInput:
            .failed(code: .invalidResponse, safeDetail: "Agent model request is invalid")
        case .http(let status) where status == 401 || status == 403:
            .failed(code: .unauthorized, safeDetail: "Agent model provider rejected access")
        case .http(let status) where status == 408 || status == 504:
            .failed(code: .timedOut, safeDetail: "Agent model provider timed out")
        case .http(let status) where status >= 500:
            .failed(code: .providerUnavailable, safeDetail: "Agent model provider unavailable")
        case .http:
            .failed(code: .invalidResponse, safeDetail: "Agent model provider rejected request")
        case .malformedResponse:
            .failed(code: .invalidResponse, safeDetail: "Agent model response is malformed")
        }
    }
}

@MainActor
final class LiveCoreAgentModelTransport: CoreAgentModelTransporting {
    func complete(
        messages: [[String: Any]],
        tools: [[String: Any]],
        model: String,
        ollamaChatURL: URL?,
        onPartialContent: @escaping (String) -> Void
    ) async throws -> AgentResult {
        try await AgentLLMClient.streamCompletion(
            messages: messages,
            tools: tools,
            model: model,
            ollamaChatURL: ollamaChatURL,
            onPartialContent: onPartialContent
        )
    }
}

@MainActor
final class UnavailableCoreAgentHost: CoreAgentHosting {
    func execute(_ request: HostRequest) async -> HostObservation {
        switch request {
        case .executeAgentCapability(let capability):
            .agentCapabilityObserved(
                turnId: capability.turnId,
                proposalId: capability.proposalId,
                executionFenceId: capability.executionFenceId,
                outcome: .failed(safeDetail: "Native agent host is unavailable")
            )
        default:
            .failed(code: .platformFailure, safeDetail: "Native agent host is unavailable")
        }
    }
}
