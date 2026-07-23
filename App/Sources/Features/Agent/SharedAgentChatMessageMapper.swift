import Foundation
import Pod0Core

@MainActor
enum SharedAgentChatMessageMapper {
    typealias MetadataResolver = @MainActor (UUID) -> RecallEvidenceMetadata?

    static func messages(
        from turns: [AgentTurnProjection],
        metadata: @escaping MetadataResolver = { _ in nil }
    ) -> [ChatMessage] {
        turns.reversed().flatMap { messages(from: $0, metadata: metadata) }
    }

    private static func messages(
        from turn: AgentTurnProjection,
        metadata: MetadataResolver
    ) -> [ChatMessage] {
        let evidence = RecallEvidenceProjectionMapper.evidence(
            from: turn.recallEvidence,
            metadata: metadata
        ) ?? []
        let finalAssistantIndex = turn.messages.lastIndex { $0.role == .assistant }
        var result = turn.messages.enumerated().map { index, message in
            ChatMessage(
                id: turn.turnId.messageUUID(at: index),
                role: role(message.role, id: turn.turnId.messageUUID(at: index)),
                text: message.role == .tool ? "Agent action completed" : message.content,
                timestamp: turn.updatedAt.date,
                recallAnswer: index == finalAssistantIndex && !evidence.isEmpty
                    ? RecallAnswer(text: message.content, evidence: evidence, status: .ready)
                    : nil
            )
        }
        if let safeFailure = turn.safeFailure, turn.stage.isVisibleFailure {
            result.append(ChatMessage(
                id: turn.turnId.messageUUID(at: turn.messages.count),
                role: .error,
                text: safeFailure,
                timestamp: turn.updatedAt.date
            ))
        }
        return result
    }

    private static func role(_ role: AgentMessageRole, id: UUID) -> ChatMessage.Role {
        switch role {
        case .user: .user
        case .assistant: .assistant
        case .tool: .toolBatch(batchID: id, count: 1)
        }
    }
}

private extension AgentTurnStage {
    var isVisibleFailure: Bool {
        switch self {
        case .blocked, .outcomeAmbiguous, .failed:
            true
        default:
            false
        }
    }
}
