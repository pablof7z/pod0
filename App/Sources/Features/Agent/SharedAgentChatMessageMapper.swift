import Foundation
import Pod0Core

enum SharedAgentChatMessageMapper {
    static func messages(from turns: [AgentTurnProjection]) -> [ChatMessage] {
        turns.reversed().flatMap(messages(from:))
    }

    private static func messages(from turn: AgentTurnProjection) -> [ChatMessage] {
        var result = turn.messages.enumerated().map { index, message in
            ChatMessage(
                id: turn.turnId.messageUUID(at: index),
                role: role(message.role, id: turn.turnId.messageUUID(at: index)),
                text: message.role == .tool ? "Agent action completed" : message.content,
                timestamp: turn.updatedAt.date
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
