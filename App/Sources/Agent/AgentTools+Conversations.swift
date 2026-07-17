import Foundation

// MARK: - Conversation history tools
//
// Skill-gated by `conversation_history`. Both handlers run on @MainActor so
// they can access `ChatHistoryStore.shared` (in-app threads).
//
// Search is lexical (case-insensitive substring) rather than embedding-based.
// In-app history is capped at 50 × 100 messages, so a full-text scan is fast
// and avoids the cost of maintaining a separate vector index for
// conversations.

extension AgentTools {

    // MARK: - Dispatcher

    @MainActor
    static func dispatchConversations(
        name: String,
        args: [String: Any],
        store: AppStateStore
    ) async -> String {
        switch name {
        case Names.listConversations:
            return listConversationsTool(args: args, store: store)
        case Names.searchConversations:
            return searchConversationsTool(args: args, store: store)
        default:
            return toolError("Unknown conversation tool: \(name)")
        }
    }

    // MARK: - list_conversations

    @MainActor
    private static func listConversationsTool(
        args: [String: Any],
        store: AppStateStore
    ) -> String {
        let limit = clampedConversationLimit(args["limit"], default: 20, max: 50)

        let results = ChatHistoryStore.shared.conversations
            .prefix(limit)
            .map(serializeConversationSummary)

        return toolSuccess([
            "conversations": results,
            "count": results.count,
        ])
    }

    // MARK: - search_conversations

    @MainActor
    private static func searchConversationsTool(
        args: [String: Any],
        store: AppStateStore
    ) -> String {
        guard let query = (args["query"] as? String)?.trimmed, !query.isEmpty else {
            return toolError("Missing or empty 'query'")
        }
        let limit = clampedConversationLimit(args["limit"], default: 10, max: 25)
        let lowercasedQuery = query.lowercased()

        var hits: [[String: Any]] = []

        for conversation in ChatHistoryStore.shared.conversations {
            for message in conversation.messages {
                guard message.text.lowercased().contains(lowercasedQuery) else { continue }
                hits.append(serializeInAppHit(message: message, conversation: conversation))
                if hits.count >= limit { break }
            }
            if hits.count >= limit { break }
        }

        return toolSuccess([
            "query": query,
            "total_found": hits.count,
            "results": hits,
        ])
    }

    // MARK: - Serializers

    @MainActor
    private static func serializeConversationSummary(_ conversation: ChatConversation) -> [String: Any] {
        let userMessages = conversation.messages.filter {
            if case .user = $0.role { return true }
            return false
        }
        let assistantMessages = conversation.messages.filter {
            if case .assistant = $0.role { return true }
            return false
        }
        var row: [String: Any] = [
            "conversation_id": conversation.id.uuidString,
            "updated_at": iso8601Basic.string(from: conversation.updatedAt),
            "message_count": conversation.messages.count,
            "user_message_count": userMessages.count,
            "assistant_message_count": assistantMessages.count,
        ]
        let title = conversation.title.trimmingCharacters(in: .whitespacesAndNewlines)
        if !title.isEmpty {
            row["title"] = title
        } else {
            row["title"] = String(conversation.firstUserSnippet.prefix(80))
        }
        if let first = userMessages.first {
            row["first_user_message"] = String(first.text.prefix(200))
        }
        return row
    }

    @MainActor
    private static func serializeInAppHit(
        message: ChatMessage,
        conversation: ChatConversation
    ) -> [String: Any] {
        let roleString: String
        switch message.role {
        case .user: roleString = "user"
        case .assistant: roleString = "assistant"
        default: roleString = "other"
        }
        let title = conversation.title.trimmingCharacters(in: .whitespacesAndNewlines)
        return [
            "conversation_id": conversation.id.uuidString,
            "conversation_title": title.isEmpty ? String(conversation.firstUserSnippet.prefix(80)) : title,
            "conversation_updated_at": iso8601Basic.string(from: conversation.updatedAt),
            "role": roleString,
            "timestamp": iso8601Basic.string(from: message.timestamp),
            "snippet": String(message.text.prefix(400)),
        ]
    }

    // MARK: - Helpers

    private static func clampedConversationLimit(_ raw: Any?, default defaultValue: Int, max: Int) -> Int {
        guard let n = numericArg(raw) else { return defaultValue }
        return Swift.max(1, Swift.min(Int(n), max))
    }
}
