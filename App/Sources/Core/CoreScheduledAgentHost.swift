import Foundation
import Pod0Core

@MainActor
protocol CoreScheduledAgentHosting: AnyObject {
    func execute(_ request: ScheduledAgentExecutionRequest) async -> ScheduledAgentExecutionObservation
}

@MainActor
protocol CoreScheduledAgentTransporting: AnyObject {
    func complete(
        modelReference: String,
        context: [ScheduledAgentContextMessage],
        prompt: String
    ) async throws -> String
}

/// Executes one exact provider turn requested by Rust. It owns only transient
/// transport state and credential access; Rust qualifies the durable artifact.
@MainActor
final class CoreScheduledAgentHost: CoreScheduledAgentHosting {
    private let transport: any CoreScheduledAgentTransporting

    init(transport: any CoreScheduledAgentTransporting = LiveCoreScheduledAgentTransport()) {
        self.transport = transport
    }

    func execute(
        _ request: ScheduledAgentExecutionRequest
    ) async -> ScheduledAgentExecutionObservation {
        do {
            let output = try await transport.complete(
                modelReference: request.modelReference,
                context: request.context,
                prompt: request.prompt
            )
            try Task.checkCancellation()
            guard UInt64(output.utf8.count) <= request.maximumOutputBytes,
                  let completion = qualifyScheduledAgentCompletion(
                    execution: request,
                    rawOutput: output
                  )
            else {
                return failure(
                    request,
                    code: .invalidOutput,
                    detail: "Provider output did not satisfy the bounded completion contract"
                )
            }
            return completion
        } catch is CancellationError {
            return .cancelled(
                occurrenceId: request.occurrenceId,
                attemptId: request.attemptId
            )
        } catch {
            let failure = ProductFailure.classify(error)
            return self.failure(
                request,
                code: Self.failureCode(failure.code),
                detail: failure.diagnosticSummary
            )
        }
    }

    private func failure(
        _ request: ScheduledAgentExecutionRequest,
        code: ScheduledAgentFailureCode,
        detail: String
    ) -> ScheduledAgentExecutionObservation {
        .failed(
            occurrenceId: request.occurrenceId,
            attemptId: request.attemptId,
            code: code,
            safeDetail: String(detail.prefix(1_024)),
            retryAfterMilliseconds: nil
        )
    }

    private static func failureCode(_ code: ProductFailureCode) -> ScheduledAgentFailureCode {
        switch code {
        case .missingCredential: .missingCredential
        case .permissionDenied: .permissionDenied
        case .rateLimited: .rateLimited
        case .offline: .offline
        case .network: .network
        case .cancelled: .cancelled
        case .unsupportedFormat, .corruptArtifact, .invalidInput: .invalidOutput
        case .providerRecovery, .missingDependency: .providerUnavailable
        case .unexpected: .unexpected
        }
    }
}

@MainActor
private final class LiveCoreScheduledAgentTransport: CoreScheduledAgentTransporting {
    func complete(
        modelReference: String,
        context: [ScheduledAgentContextMessage],
        prompt: String
    ) async throws -> String {
        var messages: [[String: Any]] = []
        for value in context {
            messages.append(try Self.message(value))
        }
        messages.append(["role": "user", "content": prompt])
        let result = try await AgentLLMClient.streamCompletion(
            messages: messages,
            tools: [],
            model: modelReference,
            feature: CostFeature.agentChat,
            onPartialContent: { _ in }
        )
        guard result.toolCalls.isEmpty,
              let output = result.assistantMessage["content"] as? String,
              !output.isBlank
        else {
            throw AgentError.malformedResponse
        }
        return output
    }

    private static func message(_ value: ScheduledAgentContextMessage) throws -> [String: Any] {
        let role = switch value.role {
        case .system: "system"
        case .user: "user"
        case .assistant: "assistant"
        case .tool: "tool"
        case .unsupported: throw AgentError.invalidInput
        }
        return ["role": role, "content": value.content]
    }
}
