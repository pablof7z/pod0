import Foundation
import Pod0Core

enum LegacyAgentHistoryMappingError: Error {
    case invalidConversation(UUID)
    case invalidMessage(UUID)
}

enum LegacyAgentHistoryMapper {
    static func map(
        _ backup: LegacyAgentHistoryBackup
    ) throws -> [LegacyAgentHistoryConversationInput] {
        try backup.conversations
            .filter { !$0.isScheduledTask && !$0.messages.isEmpty }
            .sorted { $0.id.uuidString < $1.id.uuidString }
            .map(mapConversation)
    }
}

private extension LegacyAgentHistoryMapper {
    static func mapConversation(
        _ conversation: ChatConversation
    ) throws -> LegacyAgentHistoryConversationInput {
        var turns: [LegacyAgentHistoryTurnInput] = []
        var current: [ChatMessage] = []
        for message in conversation.messages {
            if case .user = message.role {
                if !current.isEmpty {
                    turns.append(try mapTurn(current, conversationID: conversation.id))
                }
                current = [message]
            } else {
                guard !current.isEmpty else {
                    throw LegacyAgentHistoryMappingError.invalidConversation(conversation.id)
                }
                current.append(message)
            }
        }
        if !current.isEmpty {
            turns.append(try mapTurn(current, conversationID: conversation.id))
        }
        guard !turns.isEmpty,
              conversation.createdAt.timeIntervalSince1970 >= 0,
              conversation.updatedAt >= conversation.createdAt,
              turns.allSatisfy({
                  $0.createdAt.value >= UnixTimestampMilliseconds(
                      date: conversation.createdAt
                  ).value
                  && $0.updatedAt.value <= UnixTimestampMilliseconds(
                      date: conversation.updatedAt
                  ).value
              })
        else {
            throw LegacyAgentHistoryMappingError.invalidConversation(conversation.id)
        }
        return LegacyAgentHistoryConversationInput(
            conversationId: ConversationId(uuid: conversation.id),
            title: conversation.title,
            createdAt: UnixTimestampMilliseconds(date: conversation.createdAt),
            updatedAt: UnixTimestampMilliseconds(date: conversation.updatedAt),
            turns: turns
        )
    }

    static func mapTurn(
        _ messages: [ChatMessage],
        conversationID: UUID
    ) throws -> LegacyAgentHistoryTurnInput {
        guard let first = messages.first,
              case .user = first.role,
              messages.allSatisfy({ !$0.text.isEmpty }),
              messages.map(\.timestamp).allSatisfy({
                  $0.timeIntervalSince1970 >= 0
              })
        else { throw LegacyAgentHistoryMappingError.invalidMessage(conversationID) }
        return LegacyAgentHistoryTurnInput(
            turnId: AgentTurnId(uuid: first.id),
            createdAt: UnixTimestampMilliseconds(date: first.timestamp),
            updatedAt: UnixTimestampMilliseconds(
                date: messages.map(\.timestamp).max() ?? first.timestamp
            ),
            messages: messages.map {
                LegacyAgentHistoryMessageInput(role: role($0.role), content: $0.text)
            }
        )
    }

    static func role(_ role: ChatMessage.Role) -> AgentMessageRole {
        switch role {
        case .user: .user
        case .assistant: .assistant
        case .error: .error
        case .toolBatch, .skillActivated: .tool
        }
    }
}
